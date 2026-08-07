// Copyright (c) Microsoft Corporation.
// Licensed under the MIT License.

use super::check;
use expect_test::expect;
use indoc::indoc;

#[test]
fn heralded_erase_yields_unsupported_error() {
    let source = "HERALDED_ERASE(0.01) 0";
    check(
        source,
        &expect![[r#"
            Qdk.Stim.Compiler.UnsupportedInstruction

              x unsupported instruction: HERALDED_ERASE
               ,----
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
            Qdk.Stim.Compiler.UnsupportedInstruction

              x unsupported instruction: HERALDED_PAULI_CHANNEL_1
               ,----
             1 | HERALDED_PAULI_CHANNEL_1(0, 0, 0, 0.1) 0
               : ^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^
               `----
        "#]],
    );
}

#[test]
fn spp_yields_unsupported_error() {
    let source = indoc! {"
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
    "};
    check(
        source,
        &expect![[r#"
            Qdk.Stim.Compiler.UnsupportedInstruction

              x unsupported instruction: SPP
               ,-[2:1]
             1 | # Perform an S gate on qubit 1.
             2 | SPP Z1
               : ^^^^^^
             3 | 
               `----

            Qdk.Stim.Compiler.UnsupportedInstruction

              x unsupported instruction: SPP
               ,-[5:1]
             4 | # Perform a SQRT_X gate on qubit 1.
             5 | SPP X1
               : ^^^^^^
             6 | 
               `----

            Qdk.Stim.Compiler.UnsupportedInstruction

              x unsupported instruction: SPP
               ,-[8:1]
             7 | # Perform a SQRT_X_DAG gate on qubit 1.
             8 | SPP !X1
               : ^^^^^^^
             9 | 
               `----

            Qdk.Stim.Compiler.UnsupportedInstruction

              x unsupported instruction: SPP
                ,-[11:1]
             10 | # Perform a SQRT_XX gate between qubit 1 and qubit 2.
             11 | SPP X1*X2
                : ^^^^^^^^^
             12 | 
                `----

            Qdk.Stim.Compiler.UnsupportedInstruction

              x unsupported instruction: SPP
                ,-[14:1]
             13 | # Perform a SQRT_YY gate between qubit 1 and 2, and a SQRT_ZZ_DAG between qubit 3 and 4.
             14 | SPP Y1*Y2 !Z1*Z2
                : ^^^^^^^^^^^^^^^^
             15 | 
                `----

            Qdk.Stim.Compiler.UnsupportedInstruction

              x unsupported instruction: SPP
                ,-[17:1]
             16 | # Phase the -1 eigenspace of -X1*Y2*Z3 by i.
             17 | SPP !X1*Y2*Z3
                : ^^^^^^^^^^^^^
                `----
        "#]],
    );
}

#[test]
fn spp_dag_yields_unsupported_error() {
    let source = indoc! {"
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
    "};
    check(
        source,
        &expect![[r#"
            Qdk.Stim.Compiler.UnsupportedInstruction

              x unsupported instruction: SPP_DAG
               ,-[2:1]
             1 | # Perform an S_DAG gate on qubit 1.
             2 | SPP_DAG Z1
               : ^^^^^^^^^^
             3 | 
               `----

            Qdk.Stim.Compiler.UnsupportedInstruction

              x unsupported instruction: SPP_DAG
               ,-[5:1]
             4 | # Perform a SQRT_X_DAG gate on qubit 1.
             5 | SPP_DAG X1
               : ^^^^^^^^^^
             6 | 
               `----

            Qdk.Stim.Compiler.UnsupportedInstruction

              x unsupported instruction: SPP_DAG
               ,-[8:1]
             7 | # Perform a SQRT_X gate on qubit 1.
             8 | SPP_DAG !X1
               : ^^^^^^^^^^^
             9 | 
               `----

            Qdk.Stim.Compiler.UnsupportedInstruction

              x unsupported instruction: SPP_DAG
                ,-[11:1]
             10 | # Perform a SQRT_XX_DAG gate between qubit 1 and qubit 2.
             11 | SPP_DAG X1*X2
                : ^^^^^^^^^^^^^
             12 | 
                `----

            Qdk.Stim.Compiler.UnsupportedInstruction

              x unsupported instruction: SPP_DAG
                ,-[14:1]
             13 | # Perform a SQRT_YY_DAG gate between qubit 1 and 2, and a SQRT_ZZ between qubit 3 and 4.
             14 | SPP_DAG Y1*Y2 !Z1*Z2
                : ^^^^^^^^^^^^^^^^^^^^
             15 | 
                `----

            Qdk.Stim.Compiler.UnsupportedInstruction

              x unsupported instruction: SPP_DAG
                ,-[17:1]
             16 | # Phase the -1 eigenspace of -X1*Y2*Z3 by -i.
             17 | SPP_DAG !X1*Y2*Z3
                : ^^^^^^^^^^^^^^^^^
                `----
        "#]],
    );
}
