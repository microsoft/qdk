// Copyright (c) Microsoft Corporation.
// Licensed under the MIT License.

use super::check;
use expect_test::expect;

#[test]
fn qubit_target() {
    check(
        "H 0",
        &expect![[r#"
        Circuit [0-3]:
            items:
                Instruction [0-3]:
                    name: H
                    tag: <none>
                    args: <empty>
                    targets:
                        Target [2-3]:
                            kind: Qubit(0)"#]],
    );
}

#[test]
fn multiple_qubit_targets() {
    check(
        "CX 0 1 2 3",
        &expect![[r#"
        Circuit [0-10]:
            items:
                Instruction [0-10]:
                    name: CX
                    tag: <none>
                    args: <empty>
                    targets:
                        Target [3-4]:
                            kind: Qubit(0)
                        Target [5-6]:
                            kind: Qubit(1)
                        Target [7-8]:
                            kind: Qubit(2)
                        Target [9-10]:
                            kind: Qubit(3)"#]],
    );
}

#[test]
fn negated_qubit_target() {
    check(
        "M !0",
        &expect![[r#"
        Circuit [0-4]:
            items:
                Instruction [0-4]:
                    name: M
                    tag: <none>
                    args: <empty>
                    targets:
                        Target [2-4]:
                            kind: Qubit(-0)"#]],
    );
}

#[test]
fn measurement_record_target() {
    check(
        "DETECTOR rec[-1]",
        &expect![[r#"
        Circuit [0-16]:
            items:
                Instruction [0-16]:
                    name: DETECTOR
                    tag: <none>
                    args: <empty>
                    targets:
                        Target [9-16]:
                            kind: MeasurementRecord(1)"#]],
    );
}

#[test]
fn measurement_record_zero_is_error() {
    // rec[-0] is not a valid measurement record; the most recent is rec[-1].
    check(
        "DETECTOR rec[-0]",
        &expect![[r#"
            Qdk.Stim.Parser.ZeroMeasurementRecord

              x measurement record offset cannot be zero; the most recent measurement is
              | rec[-1]
               ,----
             1 | DETECTOR rec[-0]
               :               ^
               `----
        "#]],
    );
}

#[test]
fn measurement_record_zero_with_leading_zeros_is_error() {
    // rec[-00] still resolves to offset 0 and must be rejected.
    check(
        "DETECTOR rec[-00]",
        &expect![[r#"
            Qdk.Stim.Parser.ZeroMeasurementRecord

              x measurement record offset cannot be zero; the most recent measurement is
              | rec[-1]
               ,----
             1 | DETECTOR rec[-00]
               :               ^^
               `----
        "#]],
    );
}

#[test]
fn sweep_bit_target() {
    check(
        "CX sweep[0]",
        &expect![[r#"
        Circuit [0-11]:
            items:
                Instruction [0-11]:
                    name: CX
                    tag: <none>
                    args: <empty>
                    targets:
                        Target [3-11]:
                            kind: SweepBit(0)"#]],
    );
}

#[test]
fn pauli_target() {
    check(
        "MPP X0",
        &expect![[r#"
            Circuit [0-6]:
                items:
                    Instruction [0-6]:
                        name: MPP
                        tag: <none>
                        args: <empty>
                        targets:
                            Target [4-6]:
                                kind: Pauli(X 0)"#]],
    );
}

#[test]
fn negated_pauli_target() {
    check(
        "MPP !X0",
        &expect![[r#"
        Circuit [0-7]:
            items:
                Instruction [0-7]:
                    name: MPP
                    tag: <none>
                    args: <empty>
                    targets:
                        Target [4-7]:
                            kind: Pauli(-X 0)"#]],
    );
}

#[test]
fn pauli_product_two_factors() {
    check(
        "MPP X0*X1",
        &expect![[r#"
        Circuit [0-9]:
            items:
                Instruction [0-9]:
                    name: MPP
                    tag: <none>
                    args: <empty>
                    targets:
                        Target [4-9]:
                            kind: PauliProduct(Pauli(X 0)*Pauli(X 1))"#]],
    );
}

#[test]
fn pauli_product_three_factors() {
    check(
        "MPP X0*X1*X2",
        &expect![[r#"
            Circuit [0-12]:
                items:
                    Instruction [0-12]:
                        name: MPP
                        tag: <none>
                        args: <empty>
                        targets:
                            Target [4-12]:
                                kind: PauliProduct(Pauli(X 0)*Pauli(X 1)*Pauli(X 2))"#]],
    );
}

#[test]
fn pauli_product_mixed_paulis() {
    check(
        "MPP X1*Y2",
        &expect![[r#"
        Circuit [0-9]:
            items:
                Instruction [0-9]:
                    name: MPP
                    tag: <none>
                    args: <empty>
                    targets:
                        Target [4-9]:
                            kind: PauliProduct(Pauli(X 1)*Pauli(Y 2))"#]],
    );
}

#[test]
fn pauli_product_negated_first_factor() {
    check(
        "MPP !X0*Y1",
        &expect![[r#"
        Circuit [0-10]:
            items:
                Instruction [0-10]:
                    name: MPP
                    tag: <none>
                    args: <empty>
                    targets:
                        Target [4-10]:
                            kind: PauliProduct(-Pauli(X 0)*Pauli(Y 1))"#]],
    );
}

#[test]
fn pauli_product_negated_second_factor() {
    check(
        "MPP X0*!Y1",
        &expect![[r#"
        Circuit [0-10]:
            items:
                Instruction [0-10]:
                    name: MPP
                    tag: <none>
                    args: <empty>
                    targets:
                        Target [4-10]:
                            kind: PauliProduct(-Pauli(X 0)*Pauli(Y 1))"#]],
    );
}

#[test]
fn pauli_product_double_negation_cancels() {
    check(
        "MPP !X0*!Y1",
        &expect![[r#"
        Circuit [0-11]:
            items:
                Instruction [0-11]:
                    name: MPP
                    tag: <none>
                    args: <empty>
                    targets:
                        Target [4-11]:
                            kind: PauliProduct(Pauli(X 0)*Pauli(Y 1))"#]],
    );
}

#[test]
fn multiple_pauli_products() {
    check(
        "MPP X0*Y1 Z2*Z3",
        &expect![[r#"
        Circuit [0-15]:
            items:
                Instruction [0-15]:
                    name: MPP
                    tag: <none>
                    args: <empty>
                    targets:
                        Target [4-9]:
                            kind: PauliProduct(Pauli(X 0)*Pauli(Y 1))
                        Target [10-15]:
                            kind: PauliProduct(Pauli(Z 2)*Pauli(Z 3))"#]],
    );
}

#[test]
fn pauli_product_and_single_pauli() {
    check(
        "MPP X0 Y1*Z2",
        &expect![[r#"
        Circuit [0-12]:
            items:
                Instruction [0-12]:
                    name: MPP
                    tag: <none>
                    args: <empty>
                    targets:
                        Target [4-6]:
                            kind: Pauli(X 0)
                        Target [7-12]:
                            kind: PauliProduct(Pauli(Y 1)*Pauli(Z 2))"#]],
    );
}

#[test]
fn pauli_product_trailing_combiner_is_error() {
    check(
        "MPP X0*\n",
        &expect![[r#"
        Qdk.Stim.Parser.ExpectedToken

          x expected pauli_target, found newline
           ,----
         1 | MPP X0*
           :        ^
           `----
    "#]],
    );
}

#[test]
fn pauli_product_double_combiner_is_error() {
    check(
        "MPP X0**Y1",
        &expect![[r#"
        Qdk.Stim.Parser.ExpectedToken

          x expected pauli_target, found star
           ,----
         1 | MPP X0**Y1
           :        ^
           `----
    "#]],
    );
}

#[test]
fn pauli_product_with_loss_factor_is_error() {
    check(
        "MPP X0*L1",
        &expect![[r#"
        Qdk.Stim.Parser.ExpectedToken

          x expected pauli_target, found loss_target
           ,----
         1 | MPP X0*L1
           :        ^^
           `----
    "#]],
    );
}

#[test]
fn leading_combiner_is_error() {
    check(
        "MPP *X0",
        &expect![[r#"
        Qdk.Stim.Parser.Expected

          x expected a valid target, found star
           ,----
         1 | MPP *X0
           :     ^
           `----
    "#]],
    );
}

#[test]
fn bare_combiner_is_error() {
    check(
        "MPP *",
        &expect![[r#"
        Qdk.Stim.Parser.Expected

          x expected a valid target, found star
           ,----
         1 | MPP *
           :     ^
           `----
    "#]],
    );
}

#[test]
fn combiner_after_qubit_is_error() {
    check(
        "H 0*1",
        &expect![[r#"
        Qdk.Stim.Parser.Expected

          x expected a valid target, found star
           ,----
         1 | H 0*1
           :    ^
           `----
    "#]],
    );
}

#[test]
fn combiner_after_loss_is_error() {
    check(
        "E(0.01) L0*L1",
        &expect![[r#"
        Qdk.Stim.Parser.Expected

          x expected a valid target, found star
           ,----
         1 | E(0.01) L0*L1
           :           ^
           `----
    "#]],
    );
}

#[test]
fn space_between_combiners_is_ignored() {
    check(
        "MPP X0 * Y1",
        &expect![[r#"
        Circuit [0-11]:
            items:
                Instruction [0-11]:
                    name: MPP
                    tag: <none>
                    args: <empty>
                    targets:
                        Target [4-11]:
                            kind: PauliProduct(Pauli(X 0)*Pauli(Y 1))"#]],
    );
}

#[test]
fn loss_target() {
    check(
        "E(0.01) L0",
        &expect![[r#"
        Circuit [0-10]:
            items:
                Instruction [0-10]:
                    name: E
                    tag: <none>
                    args:
                        0.01
                    targets:
                        Target [8-10]:
                            kind: Loss(0)"#]],
    );
}

#[test]
fn negating_sweep_bit_is_error() {
    check(
        "CX !sweep[0] 1",
        &expect![[r#"
            Qdk.Stim.Parser.CannotNegateTarget

              x only qubit and Pauli targets can be negated with '!'
               ,----
             1 | CX !sweep[0] 1
               :    ^
               `----
        "#]],
    );
}

#[test]
fn negating_loss_is_error() {
    check(
        "E(0.01) !L0",
        &expect![[r#"
            Qdk.Stim.Parser.CannotNegateTarget

              x only qubit and Pauli targets can be negated with '!'
               ,----
             1 | E(0.01) !L0
               :         ^
               `----
        "#]],
    );
}

#[test]
fn negating_combiner_is_error() {
    check(
        "MPP X0 !*",
        &expect![[r#"
            Qdk.Stim.Parser.CannotNegateTarget

              x only qubit and Pauli targets can be negated with '!'
               ,----
             1 | MPP X0 !*
               :         ^
               `----
        "#]],
    );
}

#[test]
fn instruction_name_as_target_is_error() {
    check(
        "MPP XY",
        &expect![[r#"
            Qdk.Stim.Parser.Expected

              x expected a valid target, found instruction_name
               ,----
             1 | MPP XY
               :     ^^
               `----
        "#]],
    );
}

#[test]
fn unexpected_token_after_targets_is_error() {
    check(
        "H 0 )",
        &expect![[r#"
            Qdk.Stim.Parser.ExpectedToken

              x expected newline, found close(paren)
               ,----
             1 | H 0 )
               :     ^
               `----
        "#]],
    );
}
