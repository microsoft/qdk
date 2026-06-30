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
    check("DETECTOR rec[-0]", &expect![[r#"
        Stim.Parser.ZeroMeasurementRecord

          x measurement record offset cannot be zero; the most recent measurement is
          | rec[-1]
           ,----
         1 | DETECTOR rec[-0]
           :               ^
           `----
    "#]]);
}

#[test]
fn measurement_record_zero_with_leading_zeros_is_error() {
    // rec[-00] still resolves to offset 0 and must be rejected.
    check("DETECTOR rec[-00]", &expect![[r#"
        Stim.Parser.ZeroMeasurementRecord

          x measurement record offset cannot be zero; the most recent measurement is
          | rec[-1]
           ,----
         1 | DETECTOR rec[-00]
           :               ^^
           `----
    "#]]);
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
fn combiner_between_paulis() {
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
                        Target [4-6]:
                            kind: Pauli(X 0)
                        Target [6-7]:
                            kind: Combiner
                        Target [7-9]:
                            kind: Pauli(X 1)"#]],
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
        Stim.Parser.CannotNegateTarget

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
        Stim.Parser.CannotNegateTarget

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
        Stim.Parser.CannotNegateTarget

          x only qubit and Pauli targets can be negated with '!'
           ,----
         1 | MPP X0 !*
           :        ^
           `----
    "#]],
    );
}

#[test]
fn pauli_with_non_integer_value_is_error() {
    check(
        "MPP XY",
        &expect![[r#"
        Stim.Parser.Expected

          x expected an integer, found instruction_name
           ,----
         1 | MPP XY
           :      ^
           `----
    "#]],
    );
}

#[test]
fn unexpected_token_after_targets_is_error() {
    check(
        "H 0 )",
        &expect![[r#"
        Stim.Parser.ExpectedToken

          x expected newline, found close(paren)
           ,----
         1 | H 0 )
           :     ^
           `----
    "#]],
    );
}
