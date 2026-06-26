// Copyright (c) Microsoft Corporation.
// Licensed under the MIT License.

use super::check;
use expect_test::expect;

#[test]
fn heralded_erase_yields_unsupported_error() {
    let source = "HERALDED_ERASE(0.01) 0";
    check(
        source,
        &expect![[r#"
            Stim.UnsupportedInstruction

              x unsupported instruction: HERALDED_ERASE
               ,-[circuit:1:1]
             1 | HERALDED_ERASE(0.01) 0
               : ^^^^^^^^^^^^^^^^^^^^^^
               `----
        "#]],
    );
}

#[test]
fn heralded_pauli_channel_1_yields_unsupported_error() {
    let source = "HERALDED_PAULI_CHANNEL_1(0, 0, 0, 0.1) 0";
    check(
        source,
        &expect![[r#"
            Stim.UnsupportedInstruction

              x unsupported instruction: HERALDED_PAULI_CHANNEL_1
               ,-[circuit:1:1]
             1 | HERALDED_PAULI_CHANNEL_1(0, 0, 0, 0.1) 0
               : ^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^
               `----
        "#]],
    );
}

#[test]
fn i_error_yields_unsupported_error() {
    let source = "
# does nothing
I_ERROR 0

# does nothing with probability 0.1, else does nothing
I_ERROR(0.1) 0

# doesn't require a probability argument
I_ERROR[LEAKAGE_NOISE_FOR_AN_ADVANCED_SIMULATOR:0.1] 0

# checks for you that the disjoint probabilities in the arguments are legal
I_ERROR[MULTIPLE_NOISE_MECHANISMS](0.1, 0.2) 0
";
    check(
        source,
        &expect![[r#"
            Stim.UnsupportedInstruction

              x unsupported instruction: I_ERROR
               ,-[circuit:3:1]
             2 | # does nothing
             3 | I_ERROR 0
               : ^^^^^^^^^
             4 | 
               `----

            Stim.UnsupportedInstruction

              x unsupported instruction: I_ERROR
               ,-[circuit:6:1]
             5 | # does nothing with probability 0.1, else does nothing
             6 | I_ERROR(0.1) 0
               : ^^^^^^^^^^^^^^
             7 | 
               `----

            Stim.UnsupportedInstruction

              x unsupported instruction: I_ERROR
                ,-[circuit:9:1]
              8 | # doesn't require a probability argument
              9 | I_ERROR[LEAKAGE_NOISE_FOR_AN_ADVANCED_SIMULATOR:0.1] 0
                : ^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^
             10 | 
                `----

            Stim.UnsupportedInstruction

              x unsupported instruction: I_ERROR
                ,-[circuit:12:1]
             11 | # checks for you that the disjoint probabilities in the arguments are legal
             12 | I_ERROR[MULTIPLE_NOISE_MECHANISMS](0.1, 0.2) 0
                : ^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^
                `----
        "#]],
    );
}

#[test]
fn ii_error_yields_unsupported_error() {
    let source = "
# does nothing
II_ERROR 0 1

# does nothing with probability 0.1, else does nothing
II_ERROR(0.1) 0 1

# checks for you that the targets are two-qubit pairs
II_ERROR[TWO_QUBIT_LEAKAGE_NOISE_FOR_AN_ADVANCED_SIMULATOR:0.1] 0 2 4 6

# checks for you that the disjoint probabilities in the arguments are legal
II_ERROR[MULTIPLE_TWO_QUBIT_NOISE_MECHANISMS](0.1, 0.2) 0 2 4 6
";
    check(
        source,
        &expect![[r#"
            Stim.UnsupportedInstruction

              x unsupported instruction: II_ERROR
               ,-[circuit:3:1]
             2 | # does nothing
             3 | II_ERROR 0 1
               : ^^^^^^^^^^^^
             4 | 
               `----

            Stim.UnsupportedInstruction

              x unsupported instruction: II_ERROR
               ,-[circuit:6:1]
             5 | # does nothing with probability 0.1, else does nothing
             6 | II_ERROR(0.1) 0 1
               : ^^^^^^^^^^^^^^^^^
             7 | 
               `----

            Stim.UnsupportedInstruction

              x unsupported instruction: II_ERROR
                ,-[circuit:9:1]
              8 | # checks for you that the targets are two-qubit pairs
              9 | II_ERROR[TWO_QUBIT_LEAKAGE_NOISE_FOR_AN_ADVANCED_SIMULATOR:0.1] 0 2 4 6
                : ^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^
             10 | 
                `----

            Stim.UnsupportedInstruction

              x unsupported instruction: II_ERROR
                ,-[circuit:12:1]
             11 | # checks for you that the disjoint probabilities in the arguments are legal
             12 | II_ERROR[MULTIPLE_TWO_QUBIT_NOISE_MECHANISMS](0.1, 0.2) 0 2 4 6
                : ^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^
                `----
        "#]],
    );
}

#[test]
fn pauli_channel_1_yields_unsupported_error() {
    let source = "PAULI_CHANNEL_1(0.1, 0.2, 0.3) 0";
    check(
        source,
        &expect![[r#"
            Stim.UnsupportedInstruction

              x unsupported instruction: PAULI_CHANNEL_1
               ,-[circuit:1:1]
             1 | PAULI_CHANNEL_1(0.1, 0.2, 0.3) 0
               : ^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^
               `----
        "#]],
    );
}

#[test]
fn pauli_channel_2_yields_unsupported_error() {
    let source = "PAULI_CHANNEL_2(0,0,0, 0,0.1,0,0, 0,0,0,0.2, 0,0,0,0) 0 1";
    check(
        source,
        &expect![[r#"
            Stim.UnsupportedInstruction

              x unsupported instruction: PAULI_CHANNEL_2
               ,-[circuit:1:1]
             1 | PAULI_CHANNEL_2(0,0,0, 0,0.1,0,0, 0,0,0,0.2, 0,0,0,0) 0 1
               : ^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^
               `----
        "#]],
    );
}

#[test]
fn mpp_yields_unsupported_error() {
    let source = "
# Measure the two-body +X1*Y2 observable.
MPP X1*Y2

# Measure the one-body -Z5 observable.
MPP !Z5

# Measure the two-body +X1*Y2 observable and also the three-body -Z3*Z4*Z5 observable.
MPP X1*Y2 !Z3*Z4*Z5

# Noisily measure +Z1+Z2 and +X1*X2 (independently flip each reported result 0.1% of the time).
MPP(0.001) Z1*Z2 X1*X2
";
    check(
        source,
        &expect![[r#"
            Stim.UnsupportedInstruction

              x unsupported instruction: MPP
               ,-[circuit:3:1]
             2 | # Measure the two-body +X1*Y2 observable.
             3 | MPP X1*Y2
               : ^^^^^^^^^
             4 | 
               `----

            Stim.UnsupportedInstruction

              x unsupported instruction: MPP
               ,-[circuit:6:1]
             5 | # Measure the one-body -Z5 observable.
             6 | MPP !Z5
               : ^^^^^^^
             7 | 
               `----

            Stim.UnsupportedInstruction

              x unsupported instruction: MPP
                ,-[circuit:9:1]
              8 | # Measure the two-body +X1*Y2 observable and also the three-body -Z3*Z4*Z5 observable.
              9 | MPP X1*Y2 !Z3*Z4*Z5
                : ^^^^^^^^^^^^^^^^^^^
             10 | 
                `----

            Stim.UnsupportedInstruction

              x unsupported instruction: MPP
                ,-[circuit:12:1]
             11 | # Noisily measure +Z1+Z2 and +X1*X2 (independently flip each reported result 0.1% of the time).
             12 | MPP(0.001) Z1*Z2 X1*X2
                : ^^^^^^^^^^^^^^^^^^^^^^
                `----
        "#]],
    );
}

#[test]
fn spp_yields_unsupported_error() {
    let source = "
# Perform an S gate on qubit 1.
SPP Z1

# Perform a SQRT_X gate on qubit 1.
SPP X1

# Perform a SQRT_X_DAG gate on qubit 1.
SPP !X1

# Perform a SQRT_XX gate between qubit 1 and qubit 2.
SPP X1*X2

# Perform a SQRT_YY gate between qubit 1 and 2, and a SQRT_ZZ_DAG between qubit 3 and 4.
SPP Y1*Y2 !Z1*Z2

# Phase the -1 eigenspace of -X1*Y2*Z3 by i.
SPP !X1*Y2*Z3
";
    check(
        source,
        &expect![[r#"
            Stim.UnsupportedInstruction

              x unsupported instruction: SPP
               ,-[circuit:3:1]
             2 | # Perform an S gate on qubit 1.
             3 | SPP Z1
               : ^^^^^^
             4 | 
               `----

            Stim.UnsupportedInstruction

              x unsupported instruction: SPP
               ,-[circuit:6:1]
             5 | # Perform a SQRT_X gate on qubit 1.
             6 | SPP X1
               : ^^^^^^
             7 | 
               `----

            Stim.UnsupportedInstruction

              x unsupported instruction: SPP
                ,-[circuit:9:1]
              8 | # Perform a SQRT_X_DAG gate on qubit 1.
              9 | SPP !X1
                : ^^^^^^^
             10 | 
                `----

            Stim.UnsupportedInstruction

              x unsupported instruction: SPP
                ,-[circuit:12:1]
             11 | # Perform a SQRT_XX gate between qubit 1 and qubit 2.
             12 | SPP X1*X2
                : ^^^^^^^^^
             13 | 
                `----

            Stim.UnsupportedInstruction

              x unsupported instruction: SPP
                ,-[circuit:15:1]
             14 | # Perform a SQRT_YY gate between qubit 1 and 2, and a SQRT_ZZ_DAG between qubit 3 and 4.
             15 | SPP Y1*Y2 !Z1*Z2
                : ^^^^^^^^^^^^^^^^
             16 | 
                `----

            Stim.UnsupportedInstruction

              x unsupported instruction: SPP
                ,-[circuit:18:1]
             17 | # Phase the -1 eigenspace of -X1*Y2*Z3 by i.
             18 | SPP !X1*Y2*Z3
                : ^^^^^^^^^^^^^
                `----
        "#]],
    );
}

#[test]
fn spp_dag_yields_unsupported_error() {
    let source = "
# Perform an S_DAG gate on qubit 1.
SPP_DAG Z1

# Perform a SQRT_X_DAG gate on qubit 1.
SPP_DAG X1

# Perform a SQRT_X gate on qubit 1.
SPP_DAG !X1

# Perform a SQRT_XX_DAG gate between qubit 1 and qubit 2.
SPP_DAG X1*X2

# Perform a SQRT_YY_DAG gate between qubit 1 and 2, and a SQRT_ZZ between qubit 3 and 4.
SPP_DAG Y1*Y2 !Z1*Z2

# Phase the -1 eigenspace of -X1*Y2*Z3 by -i.
SPP_DAG !X1*Y2*Z3
";
    check(
        source,
        &expect![[r#"
            Stim.UnsupportedInstruction

              x unsupported instruction: SPP_DAG
               ,-[circuit:3:1]
             2 | # Perform an S_DAG gate on qubit 1.
             3 | SPP_DAG Z1
               : ^^^^^^^^^^
             4 | 
               `----

            Stim.UnsupportedInstruction

              x unsupported instruction: SPP_DAG
               ,-[circuit:6:1]
             5 | # Perform a SQRT_X_DAG gate on qubit 1.
             6 | SPP_DAG X1
               : ^^^^^^^^^^
             7 | 
               `----

            Stim.UnsupportedInstruction

              x unsupported instruction: SPP_DAG
                ,-[circuit:9:1]
              8 | # Perform a SQRT_X gate on qubit 1.
              9 | SPP_DAG !X1
                : ^^^^^^^^^^^
             10 | 
                `----

            Stim.UnsupportedInstruction

              x unsupported instruction: SPP_DAG
                ,-[circuit:12:1]
             11 | # Perform a SQRT_XX_DAG gate between qubit 1 and qubit 2.
             12 | SPP_DAG X1*X2
                : ^^^^^^^^^^^^^
             13 | 
                `----

            Stim.UnsupportedInstruction

              x unsupported instruction: SPP_DAG
                ,-[circuit:15:1]
             14 | # Perform a SQRT_YY_DAG gate between qubit 1 and 2, and a SQRT_ZZ between qubit 3 and 4.
             15 | SPP_DAG Y1*Y2 !Z1*Z2
                : ^^^^^^^^^^^^^^^^^^^^
             16 | 
                `----

            Stim.UnsupportedInstruction

              x unsupported instruction: SPP_DAG
                ,-[circuit:18:1]
             17 | # Phase the -1 eigenspace of -X1*Y2*Z3 by -i.
             18 | SPP_DAG !X1*Y2*Z3
                : ^^^^^^^^^^^^^^^^^
                `----
        "#]],
    );
}

#[test]
fn repeat_yields_unsupported_error() {
    let source = "
REPEAT 10 {
    CNOT 0 1
    CNOT 2 1
    M 1
}
";
    check(
        source,
        &expect![[r#"
            Stim.UnsupportedInstruction

              x unsupported instruction: REPEAT
               ,-[circuit:2:1]
             1 | 
             2 | REPEAT 10 {
               : ^^^^^^^^^
             3 |     CNOT 0 1
               `----
        "#]],
    );
}
