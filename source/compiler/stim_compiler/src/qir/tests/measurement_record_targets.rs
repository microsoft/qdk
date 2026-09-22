// Copyright (c) Microsoft Corporation.
// Licensed under the MIT License.

use crate::qir::tests::check;
use expect_test::expect;
use indoc::indoc;

#[test]
fn cx_with_rec_control_yields_expected_qir() {
    let source = indoc! {"
        M 0
        CX rec[-1] 1
    "};
    check(
        source,
        &expect![[r#"
            [entry_point]
                call void @__quantum__qis__m__body(ptr inttoptr (i64 0 to ptr), ptr inttoptr (i64 0 to ptr))
                call void @classical_control_cx(ptr inttoptr (i64 0 to ptr), ptr inttoptr (i64 1 to ptr))

            [definitions]
              define void @classical_control_cx(ptr %result, ptr %qubit) {
              block_cx_entry:
                %result_val = call i1 @__quantum__rt__read_result(ptr %result)
                br i1 %result_val, label %block_cx_apply, label %block_cx_exit
              block_cx_apply:
                call void @__quantum__qis__x__body(ptr %qubit)
                br label %block_cx_exit
              block_cx_exit:
                ret void
              }

            [declarations]
              declare i1 @__quantum__rt__read_result(ptr)
              declare void @__quantum__qis__m__body(ptr, ptr)
              declare void @__quantum__qis__x__body(ptr)

            [metadata]
              required_num_qubits = 2
              required_num_results = 1"#]],
    );
}

#[test]
fn cx_with_older_rec_control_yields_expected_qir() {
    let source = indoc! {"
        M 0
        M 1
        CX rec[-2] 2
    "};
    check(
        source,
        &expect![[r#"
            [entry_point]
                call void @__quantum__qis__m__body(ptr inttoptr (i64 0 to ptr), ptr inttoptr (i64 0 to ptr))
                call void @__quantum__qis__m__body(ptr inttoptr (i64 1 to ptr), ptr inttoptr (i64 1 to ptr))
                call void @classical_control_cx(ptr inttoptr (i64 0 to ptr), ptr inttoptr (i64 2 to ptr))

            [definitions]
              define void @classical_control_cx(ptr %result, ptr %qubit) {
              block_cx_entry:
                %result_val = call i1 @__quantum__rt__read_result(ptr %result)
                br i1 %result_val, label %block_cx_apply, label %block_cx_exit
              block_cx_apply:
                call void @__quantum__qis__x__body(ptr %qubit)
                br label %block_cx_exit
              block_cx_exit:
                ret void
              }

            [declarations]
              declare i1 @__quantum__rt__read_result(ptr)
              declare void @__quantum__qis__m__body(ptr, ptr)
              declare void @__quantum__qis__x__body(ptr)

            [metadata]
              required_num_qubits = 3
              required_num_results = 2"#]],
    );
}

#[test]
fn cx_with_mixed_quantum_and_classical_pairs_yields_expected_qir() {
    let source = indoc! {"
        M 0
        CX rec[-1] 1 2 3
    "};
    check(
        source,
        &expect![[r#"
            [entry_point]
                call void @__quantum__qis__m__body(ptr inttoptr (i64 0 to ptr), ptr inttoptr (i64 0 to ptr))
                call void @classical_control_cx(ptr inttoptr (i64 0 to ptr), ptr inttoptr (i64 1 to ptr))
                call void @__quantum__qis__cx__body(ptr inttoptr (i64 2 to ptr), ptr inttoptr (i64 3 to ptr))

            [definitions]
              define void @classical_control_cx(ptr %result, ptr %qubit) {
              block_cx_entry:
                %result_val = call i1 @__quantum__rt__read_result(ptr %result)
                br i1 %result_val, label %block_cx_apply, label %block_cx_exit
              block_cx_apply:
                call void @__quantum__qis__x__body(ptr %qubit)
                br label %block_cx_exit
              block_cx_exit:
                ret void
              }

            [declarations]
              declare i1 @__quantum__rt__read_result(ptr)
              declare void @__quantum__qis__cx__body(ptr, ptr)
              declare void @__quantum__qis__m__body(ptr, ptr)
              declare void @__quantum__qis__x__body(ptr)

            [metadata]
              required_num_qubits = 4
              required_num_results = 1"#]],
    );
}

#[test]
fn cx_with_multiple_classical_pairs_yields_expected_qir() {
    let source = indoc! {"
        M 0
        M 1
        CX rec[-1] 2 rec[-2] 3
    "};
    check(
        source,
        &expect![[r#"
            [entry_point]
                call void @__quantum__qis__m__body(ptr inttoptr (i64 0 to ptr), ptr inttoptr (i64 0 to ptr))
                call void @__quantum__qis__m__body(ptr inttoptr (i64 1 to ptr), ptr inttoptr (i64 1 to ptr))
                call void @classical_control_cx(ptr inttoptr (i64 1 to ptr), ptr inttoptr (i64 2 to ptr))
                call void @classical_control_cx(ptr inttoptr (i64 0 to ptr), ptr inttoptr (i64 3 to ptr))

            [definitions]
              define void @classical_control_cx(ptr %result, ptr %qubit) {
              block_cx_entry:
                %result_val = call i1 @__quantum__rt__read_result(ptr %result)
                br i1 %result_val, label %block_cx_apply, label %block_cx_exit
              block_cx_apply:
                call void @__quantum__qis__x__body(ptr %qubit)
                br label %block_cx_exit
              block_cx_exit:
                ret void
              }

            [declarations]
              declare i1 @__quantum__rt__read_result(ptr)
              declare void @__quantum__qis__m__body(ptr, ptr)
              declare void @__quantum__qis__x__body(ptr)

            [metadata]
              required_num_qubits = 4
              required_num_results = 2"#]],
    );
}

#[test]
fn cy_with_rec_control_yields_expected_qir() {
    let source = indoc! {"
        M 0
        CY rec[-1] 1
    "};
    check(
        source,
        &expect![[r#"
            [entry_point]
                call void @__quantum__qis__m__body(ptr inttoptr (i64 0 to ptr), ptr inttoptr (i64 0 to ptr))
                call void @classical_control_cy(ptr inttoptr (i64 0 to ptr), ptr inttoptr (i64 1 to ptr))

            [definitions]
              define void @classical_control_cy(ptr %result, ptr %qubit) {
              block_cy_entry:
                %result_val = call i1 @__quantum__rt__read_result(ptr %result)
                br i1 %result_val, label %block_cy_apply, label %block_cy_exit
              block_cy_apply:
                call void @__quantum__qis__y__body(ptr %qubit)
                br label %block_cy_exit
              block_cy_exit:
                ret void
              }

            [declarations]
              declare i1 @__quantum__rt__read_result(ptr)
              declare void @__quantum__qis__m__body(ptr, ptr)
              declare void @__quantum__qis__y__body(ptr)

            [metadata]
              required_num_qubits = 2
              required_num_results = 1"#]],
    );
}

#[test]
fn cz_with_rec_on_first_target_yields_expected_qir() {
    let source = indoc! {"
        M 0
        CZ rec[-1] 1
    "};
    check(
        source,
        &expect![[r#"
            [entry_point]
                call void @__quantum__qis__m__body(ptr inttoptr (i64 0 to ptr), ptr inttoptr (i64 0 to ptr))
                call void @classical_control_cz(ptr inttoptr (i64 0 to ptr), ptr inttoptr (i64 1 to ptr))

            [definitions]
              define void @classical_control_cz(ptr %result, ptr %qubit) {
              block_cz_entry:
                %result_val = call i1 @__quantum__rt__read_result(ptr %result)
                br i1 %result_val, label %block_cz_apply, label %block_cz_exit
              block_cz_apply:
                call void @__quantum__qis__z__body(ptr %qubit)
                br label %block_cz_exit
              block_cz_exit:
                ret void
              }

            [declarations]
              declare i1 @__quantum__rt__read_result(ptr)
              declare void @__quantum__qis__m__body(ptr, ptr)
              declare void @__quantum__qis__z__body(ptr)

            [metadata]
              required_num_qubits = 2
              required_num_results = 1"#]],
    );
}

#[test]
fn cz_with_rec_on_second_target_yields_expected_qir() {
    let source = indoc! {"
        M 0
        CZ 0 rec[-1]
    "};
    check(
        source,
        &expect![[r#"
            [entry_point]
                call void @__quantum__qis__m__body(ptr inttoptr (i64 0 to ptr), ptr inttoptr (i64 0 to ptr))
                call void @classical_control_cz(ptr inttoptr (i64 0 to ptr), ptr inttoptr (i64 0 to ptr))

            [definitions]
              define void @classical_control_cz(ptr %result, ptr %qubit) {
              block_cz_entry:
                %result_val = call i1 @__quantum__rt__read_result(ptr %result)
                br i1 %result_val, label %block_cz_apply, label %block_cz_exit
              block_cz_apply:
                call void @__quantum__qis__z__body(ptr %qubit)
                br label %block_cz_exit
              block_cz_exit:
                ret void
              }

            [declarations]
              declare i1 @__quantum__rt__read_result(ptr)
              declare void @__quantum__qis__m__body(ptr, ptr)
              declare void @__quantum__qis__z__body(ptr)

            [metadata]
              required_num_qubits = 1
              required_num_results = 1"#]],
    );
}

#[test]
fn xcz_with_rec_on_second_target_yields_expected_qir() {
    let source = indoc! {"
        M 0
        XCZ 1 rec[-1]
    "};
    check(
        source,
        &expect![[r#"
            [entry_point]
                call void @__quantum__qis__m__body(ptr inttoptr (i64 0 to ptr), ptr inttoptr (i64 0 to ptr))
                call void @classical_control_cx(ptr inttoptr (i64 0 to ptr), ptr inttoptr (i64 1 to ptr))

            [definitions]
              define void @classical_control_cx(ptr %result, ptr %qubit) {
              block_cx_entry:
                %result_val = call i1 @__quantum__rt__read_result(ptr %result)
                br i1 %result_val, label %block_cx_apply, label %block_cx_exit
              block_cx_apply:
                call void @__quantum__qis__x__body(ptr %qubit)
                br label %block_cx_exit
              block_cx_exit:
                ret void
              }

            [declarations]
              declare i1 @__quantum__rt__read_result(ptr)
              declare void @__quantum__qis__m__body(ptr, ptr)
              declare void @__quantum__qis__x__body(ptr)

            [metadata]
              required_num_qubits = 2
              required_num_results = 1"#]],
    );
}

#[test]
fn ycz_with_rec_on_second_target_yields_expected_qir() {
    let source = indoc! {"
        M 0
        YCZ 1 rec[-1]
    "};
    check(
        source,
        &expect![[r#"
            [entry_point]
                call void @__quantum__qis__m__body(ptr inttoptr (i64 0 to ptr), ptr inttoptr (i64 0 to ptr))
                call void @classical_control_cy(ptr inttoptr (i64 0 to ptr), ptr inttoptr (i64 1 to ptr))

            [definitions]
              define void @classical_control_cy(ptr %result, ptr %qubit) {
              block_cy_entry:
                %result_val = call i1 @__quantum__rt__read_result(ptr %result)
                br i1 %result_val, label %block_cy_apply, label %block_cy_exit
              block_cy_apply:
                call void @__quantum__qis__y__body(ptr %qubit)
                br label %block_cy_exit
              block_cy_exit:
                ret void
              }

            [declarations]
              declare i1 @__quantum__rt__read_result(ptr)
              declare void @__quantum__qis__m__body(ptr, ptr)
              declare void @__quantum__qis__y__body(ptr)

            [metadata]
              required_num_qubits = 2
              required_num_results = 1"#]],
    );
}

#[test]
fn cx_with_rec_control_crossing_select_boundary() {
    let source = indoc! {"
        M 0
        SELECT {
          CX rec[-1] 1
        }
    "};
    check(
        source,
        &expect![[r#"
            [entry_point]
                call void @__quantum__qis__m__body(ptr inttoptr (i64 0 to ptr), ptr inttoptr (i64 0 to ptr))
                br label %select_0
              select_0:
                call void @classical_control_cx(ptr inttoptr (i64 0 to ptr), ptr inttoptr (i64 1 to ptr))

            [definitions]
              define void @classical_control_cx(ptr %result, ptr %qubit) {
              block_cx_entry:
                %result_val = call i1 @__quantum__rt__read_result(ptr %result)
                br i1 %result_val, label %block_cx_apply, label %block_cx_exit
              block_cx_apply:
                call void @__quantum__qis__x__body(ptr %qubit)
                br label %block_cx_exit
              block_cx_exit:
                ret void
              }

            [declarations]
              declare i1 @__quantum__rt__read_result(ptr)
              declare void @__quantum__qis__m__body(ptr, ptr)
              declare void @__quantum__qis__x__body(ptr)

            [metadata]
              required_num_qubits = 2
              required_num_results = 1"#]],
    );
}

#[test]
fn top_level_classical_control_reaches_into_select() {
    let source = indoc! {"
        SELECT {
          M 0
        }
        CX rec[-1] 1
    "};
    check(
        source,
        &expect![[r#"
            [entry_point]
                br label %select_0
              select_0:
                call void @__quantum__qis__m__body(ptr inttoptr (i64 0 to ptr), ptr inttoptr (i64 0 to ptr))
                call void @classical_control_cx(ptr inttoptr (i64 0 to ptr), ptr inttoptr (i64 1 to ptr))

            [definitions]
              define void @classical_control_cx(ptr %result, ptr %qubit) {
              block_cx_entry:
                %result_val = call i1 @__quantum__rt__read_result(ptr %result)
                br i1 %result_val, label %block_cx_apply, label %block_cx_exit
              block_cx_apply:
                call void @__quantum__qis__x__body(ptr %qubit)
                br label %block_cx_exit
              block_cx_exit:
                ret void
              }

            [declarations]
              declare i1 @__quantum__rt__read_result(ptr)
              declare void @__quantum__qis__m__body(ptr, ptr)
              declare void @__quantum__qis__x__body(ptr)

            [metadata]
              required_num_qubits = 2
              required_num_results = 1"#]],
    );
}
