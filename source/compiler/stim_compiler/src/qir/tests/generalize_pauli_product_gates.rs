// Copyright (c) Microsoft Corporation.
// Licensed under the MIT License.

use super::check;
use expect_test::expect;
use indoc::indoc;

#[test]
#[ignore = "unsupported instruction"]
fn mpp_yields_expected_qir() {
    let source = indoc! {"
        # Measure the two-body +X1*Y2 observable.
        MPP X1*Y2

        # Measure the one-body -Z5 observable.
        MPP !Z5

        # Measure the two-body +X1*Y2 observable and also the three-body -Z3*Z4*Z5 observable.
        MPP X1*Y2 !Z3*Z4*Z5

        # Noisily measure +Z1+Z2 and +X1*X2 (independently flip each reported result 0.1% of the time).
        MPP(0.001) Z1*Z2 X1*X2
    "};
    check(source, &expect![[""]]);
}

#[test]
#[ignore = "unsupported instruction"]
fn spp_yields_expected_qir() {
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
    check(source, &expect![[""]]);
}

#[test]
#[ignore = "unsupported instruction"]
fn spp_dag_yields_expected_qir() {
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
    check(source, &expect![[""]]);
}
