// Copyright (c) Microsoft Corporation.
// Licensed under the MIT License.

use crate::qir::tests::check;
use expect_test::expect;

#[test]
fn simple_prepare_block() {
    // should require result of M 0 == 0
    let source = "
PREPARE {
    M 0
    REQUIRE rec[-1]
}
";

    check(source, &expect![]);
}

#[test]
fn long_prepare_block() {
    let source = "
PREPARE {
    X 0
    M 0
    H 1
    X 1
    M 1
    M 2
    REQUIRE rec[-1] rec[-2] rec[-2]
}
";
    check(source, &expect![]);
}

#[test]
fn multiple_requires_in_block() {
    let source = "
PREPARE {
    M 0
    REQUIRE rec[-1]
    M 1
    REQUIRE rec[-1] rec[-2]
}
";
    check(source, &expect![]);
}

#[test]
fn prepare_block_no_require() {
    // should compile to a QIR that skips the prepare statement
    let source = "
PREPARE {
    M 0
    M 1
    M 2
}
";
    check(source, &expect![]);
}

#[test]
fn empty_prepare_block() {
    // should compile to a QIR that skips the prepare statement
    let source = "
PREPARE {
}
";
    check(source, &expect![]);
}

#[test]
fn prepare_block_with_args_yields_error() {
    let source = "
PREPARE(0.5) {
    M 0
    REQUIRE rec[-1]
}
";
    check(source, &expect![]);
}

#[test]
fn prepare_block_with_targets_yields_error() {
    let source = "
PREPARE 0 1 {
    M 0
    REQUIRE rec[-1]
}
";
    check(source, &expect![]);
}

#[test]
fn prepare_block_with_tag() {
    let source = "
PREPARE[some_tag] {
    M 0
    REQUIRE rec[-1]
}
";
    check(source, &expect![]);
}

#[test]
fn require_with_negated_target() {
    let source = "
PREPARE {
    M 0
    REQUIRE !rec[-1]
}
";
    check(source, &expect![]);
}

#[test]
fn require_with_integer_target_yields_error() {
    let source = "
PREPARE {
    M 0
    REQUIRE 0
}
";
    check(source, &expect![]);
}

#[test]
fn require_with_pauli_target_yields_error() {
    let source = "
PREPARE {
    M 0
    REQUIRE X0
}
";
    check(source, &expect![]);
}

#[test]
fn require_with_no_targets_yields_error() {
    let source = "
PREPARE {
    M 0
    REQUIRE
}
";
    check(source, &expect![]);
}

#[test]
fn require_no_prepare_block_yields_error() {
    let source = "
REQUIRE rec[-1]
";
    check(source, &expect![]);
}

#[test]
fn require_with_no_measurements_yields_error() {
    let source = "
PREPARE {
    REQUIRE rec[-1]
}
";
    check(source, &expect![]);
}

#[test]
fn require_before_measurement_yields_error() {
    let source = "
PREPARE {
    REQUIRE rec[-1]
    M 0
}
";
    check(source, &expect![]);
}

#[test]
fn rec_index_out_of_bounds() {
    let source = "
PREPARE {
    M 0
    REQUIRE rec[-2]
}
";
    check(source, &expect![]);
}

#[test]
fn rec_index_out_of_bounds_2() {
    let source = "
M 0
PREPARE {
    M 1
    REQUIRE rec[-2]
}
";
    check(source, &expect![]);
}

#[test]
fn reset_does_not_count_as_measurement() {
    // R does not produce a measurement record, so rec[-1] should be out of bounds.
    let source = "
PREPARE {
    R 0
    REQUIRE rec[-1]
}
";
    check(source, &expect![]);
}

#[test]
fn measure_reset_counts_as_measurement() {
    // MR produces a measurement record.
    let source = "
PREPARE {
    MR 0
    REQUIRE rec[-1]
}
";
    check(source, &expect![]);
}

#[test]
fn pair_measurement_record_in_prepare() {
    let source = "
PREPARE {
    MZZ 0 1
    REQUIRE rec[-1]
}
";
    check(source, &expect![]);
}

#[test]
fn pair_measurement_record_in_prepare_2() {
    // A two-qubit measurement produces a single measurement record.
    // So this should not be valid
    let source = "
PREPARE {
    MZZ 0 1
    REQUIRE rec[-1] rec[-2]
}
";
    check(source, &expect![]);
}

#[test]
fn nested_prepare_blocks() {
    let source = "
PREPARE {
    PREPARE {
        M 0
        REQUIRE rec[-1]
    }
    M 1
    REQUIRE rec[-1]
}
";
    check(source, &expect![]);
}

#[test]
fn deeply_nested_prepare_blocks() {
    let source = "
PREPARE {
    PREPARE {
        PREPARE {
            M 0
            REQUIRE rec[-1]
        }
        M 1
        REQUIRE rec[-1]
    }
    M 2
    REQUIRE rec[-1]
}
";
    check(source, &expect![]);
}

#[test]
fn outer_prepare_reaches_into_inner_prepare_yields_error() {
    let source = "
PREPARE {
    PREPARE {
        M 0
    }
    REQUIRE rec[-1]
}
";
    check(source, &expect![]);
}

#[test]
fn inner_prepare_reaches_into_outer_prepare_yields_error() {
    let source = "
PREPARE {
    M 0
    PREPARE {
        REQUIRE rec[-1]
    }
}
";
    check(source, &expect![]);
}
