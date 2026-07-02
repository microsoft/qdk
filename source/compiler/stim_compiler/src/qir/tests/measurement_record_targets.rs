// Copyright (c) Microsoft Corporation.
// Licensed under the MIT License.

use crate::qir::tests::check;
use expect_test::expect;

#[test]
fn cx_with_rec_control_yields_expected_qir() {
    let source = "M 0\nCX rec[-1] 1";
    check(source, &expect![[""]]);
}

#[test]
fn cnot_with_rec_control_yields_expected_qir() {
    let source = "M 0\nCNOT rec[-1] 1";
    check(source, &expect![[""]]);
}

#[test]
fn zcx_with_rec_control_yields_expected_qir() {
    let source = "M 0\nZCX rec[-1] 1";
    check(source, &expect![[""]]);
}

#[test]
fn cx_with_older_rec_control_yields_expected_qir() {
    let source = "M 0\nM 1\nCX rec[-2] 2";
    check(source, &expect![[""]]);
}

#[test]
fn cx_with_mixed_quantum_and_classical_pairs_yields_expected_qir() {
    let source = "M 0\nCX rec[-1] 1 2 3";
    check(source, &expect![[""]]);
}

#[test]
fn cx_with_multiple_classical_pairs_yields_expected_qir() {
    let source = "M 0\nM 1\nCX rec[-1] 2 rec[-2] 3";
    check(source, &expect![[""]]);
}

#[test]
fn cx_with_rec_on_second_target_yields_error() {
    let source = "M 0\nCX 0 rec[-1]";
    check(source, &expect![[""]]);
}

#[test]
fn cx_with_negated_rec_control_yields_error() {
    let source = "M 0\nCX !rec[-1] 1";
    check(source, &expect![[""]]);
}

#[test]
fn cx_with_rec_control_out_of_bounds_yields_error() {
    let source = "CX rec[-1] 1";
    check(source, &expect![[""]]);
}

#[test]
fn cx_with_two_rec_targets_yields_error() {
    let source = "M 0\nM 1\nCX rec[-1] rec[-2]";
    check(source, &expect![[""]]);
}

#[test]
fn cx_with_odd_targets_including_rec_yields_error() {
    let source = "M 0\nCX rec[-1]";
    check(source, &expect![[""]]);
}

#[test]
fn cy_with_rec_control_yields_expected_qir() {
    let source = "M 0\nCY rec[-1] 1";
    check(source, &expect![[""]]);
}

#[test]
fn zcy_with_rec_control_yields_expected_qir() {
    let source = "M 0\nZCY rec[-1] 1";
    check(source, &expect![[""]]);
}

#[test]
fn cy_with_rec_on_second_target_yields_error() {
    let source = "M 0\nCY 0 rec[-1]";
    check(source, &expect![[""]]);
}

#[test]
fn cy_with_negated_rec_control_yields_error() {
    let source = "M 0\nCY !rec[-1] 1";
    check(source, &expect![[""]]);
}

#[test]
fn cz_with_rec_on_first_target_yields_expected_qir() {
    let source = "M 0\nCZ rec[-1] 1";
    check(source, &expect![[""]]);
}

#[test]
fn cz_with_rec_on_second_target_yields_expected_qir() {
    let source = "M 0\nCZ 0 rec[-1]";
    check(source, &expect![[""]]);
}

#[test]
fn zcz_with_rec_on_first_target_yields_expected_qir() {
    let source = "M 0\nZCZ rec[-1] 1";
    check(source, &expect![[""]]);
}

#[test]
fn zcz_with_rec_on_second_target_yields_expected_qir() {
    let source = "M 0\nZCZ 0 rec[-1]";
    check(source, &expect![[""]]);
}

#[test]
fn cz_with_two_rec_targets_yields_error() {
    let source = "M 0\nM 1\nCZ rec[-1] rec[-2]";
    check(source, &expect![[""]]);
}

#[test]
fn cz_with_negated_rec_on_first_target_yields_error() {
    let source = "M 0\nCZ !rec[-1] 1";
    check(source, &expect![[""]]);
}

#[test]
fn cz_with_negated_rec_on_second_target_yields_error() {
    let source = "M 0\nCZ 0 !rec[-1]";
    check(source, &expect![[""]]);
}

#[test]
fn xcz_with_rec_on_second_target_yields_expected_qir() {
    let source = "M 0\nXCZ 1 rec[-1]";
    check(source, &expect![[""]]);
}

#[test]
fn xcz_with_rec_on_first_target_yields_error() {
    let source = "M 0\nXCZ rec[-1] 1";
    check(source, &expect![[""]]);
}

#[test]
fn xcz_with_negated_rec_on_second_target_yields_error() {
    let source = "M 0\nXCZ 1 !rec[-1]";
    check(source, &expect![[""]]);
}

#[test]
fn ycz_with_rec_on_second_target_yields_expected_qir() {
    let source = "M 0\nYCZ 1 rec[-1]";
    check(source, &expect![[""]]);
}

#[test]
fn ycz_with_rec_on_first_target_yields_error() {
    let source = "M 0\nYCZ rec[-1] 1";
    check(source, &expect![[""]]);
}

#[test]
fn ycz_with_negated_rec_on_second_target_yields_error() {
    let source = "M 0\nYCZ 1 !rec[-1]";
    check(source, &expect![[""]]);
}

#[test]
fn cx_with_rec_control_crossing_prepare_boundary_yields_error() {
    let source = "M 0\nPREPARE {\n    CX rec[-1] 1\n}";
    check(source, &expect![[""]]);
}
