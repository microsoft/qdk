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
            body:
                call void @__quantum__qis__m__body(ptr inttoptr (i64 0 to ptr), ptr inttoptr (i64 0 to ptr))
                call void @classical_control_cx(ptr inttoptr (i64 0 to ptr), ptr inttoptr (i64 1 to ptr))

            definitions:
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

            declarations:
              declare i1 @__quantum__rt__read_result(ptr)
              declare void @__quantum__qis__m__body(ptr, ptr)
              declare void @__quantum__qis__x__body(ptr)

            required_num_qubits: 2
            required_num_results: 1"#]],
    );
}

#[test]
fn cnot_with_rec_control_yields_expected_qir() {
    let source = indoc! {"
        M 0
        CNOT rec[-1] 1
    "};
    check(
        source,
        &expect![[r#"
            body:
                call void @__quantum__qis__m__body(ptr inttoptr (i64 0 to ptr), ptr inttoptr (i64 0 to ptr))
                call void @classical_control_cx(ptr inttoptr (i64 0 to ptr), ptr inttoptr (i64 1 to ptr))

            definitions:
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

            declarations:
              declare i1 @__quantum__rt__read_result(ptr)
              declare void @__quantum__qis__m__body(ptr, ptr)
              declare void @__quantum__qis__x__body(ptr)

            required_num_qubits: 2
            required_num_results: 1"#]],
    );
}

#[test]
fn zcx_with_rec_control_yields_expected_qir() {
    let source = indoc! {"
        M 0
        ZCX rec[-1] 1
    "};
    check(
        source,
        &expect![[r#"
            body:
                call void @__quantum__qis__m__body(ptr inttoptr (i64 0 to ptr), ptr inttoptr (i64 0 to ptr))
                call void @classical_control_cx(ptr inttoptr (i64 0 to ptr), ptr inttoptr (i64 1 to ptr))

            definitions:
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

            declarations:
              declare i1 @__quantum__rt__read_result(ptr)
              declare void @__quantum__qis__m__body(ptr, ptr)
              declare void @__quantum__qis__x__body(ptr)

            required_num_qubits: 2
            required_num_results: 1"#]],
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
            body:
                call void @__quantum__qis__m__body(ptr inttoptr (i64 0 to ptr), ptr inttoptr (i64 0 to ptr))
                call void @__quantum__qis__m__body(ptr inttoptr (i64 1 to ptr), ptr inttoptr (i64 1 to ptr))
                call void @classical_control_cx(ptr inttoptr (i64 0 to ptr), ptr inttoptr (i64 2 to ptr))

            definitions:
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

            declarations:
              declare i1 @__quantum__rt__read_result(ptr)
              declare void @__quantum__qis__m__body(ptr, ptr)
              declare void @__quantum__qis__x__body(ptr)

            required_num_qubits: 3
            required_num_results: 2"#]],
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
            body:
                call void @__quantum__qis__m__body(ptr inttoptr (i64 0 to ptr), ptr inttoptr (i64 0 to ptr))
                call void @classical_control_cx(ptr inttoptr (i64 0 to ptr), ptr inttoptr (i64 1 to ptr))
                call void @__quantum__qis__cx__body(ptr inttoptr (i64 2 to ptr), ptr inttoptr (i64 3 to ptr))

            definitions:
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

            declarations:
              declare i1 @__quantum__rt__read_result(ptr)
              declare void @__quantum__qis__cx__body(ptr, ptr)
              declare void @__quantum__qis__m__body(ptr, ptr)
              declare void @__quantum__qis__x__body(ptr)

            required_num_qubits: 4
            required_num_results: 1"#]],
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
            body:
                call void @__quantum__qis__m__body(ptr inttoptr (i64 0 to ptr), ptr inttoptr (i64 0 to ptr))
                call void @__quantum__qis__m__body(ptr inttoptr (i64 1 to ptr), ptr inttoptr (i64 1 to ptr))
                call void @classical_control_cx(ptr inttoptr (i64 1 to ptr), ptr inttoptr (i64 2 to ptr))
                call void @classical_control_cx(ptr inttoptr (i64 0 to ptr), ptr inttoptr (i64 3 to ptr))

            definitions:
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

            declarations:
              declare i1 @__quantum__rt__read_result(ptr)
              declare void @__quantum__qis__m__body(ptr, ptr)
              declare void @__quantum__qis__x__body(ptr)

            required_num_qubits: 4
            required_num_results: 2"#]],
    );
}

#[test]
fn cx_with_rec_on_second_target_yields_error() {
    let source = indoc! {"
        M 0
        CX 0 rec[-1]
    "};
    check(
        source,
        &expect![[r#"
            Qdk.Stim.Compiler.MisplacedMeasurementRecord

              x measurement record target in an unsupported position in instruction: CX
               ,-[2:6]
             1 | M 0
             2 | CX 0 rec[-1]
               :      ^^^^^^^
               `----
        "#]],
    );
}

#[test]
fn cx_with_negated_rec_control_yields_error() {
    let source = indoc! {"
        M 0
        CX !rec[-1] 1
    "};
    check(
        source,
        &expect![[r#"
            Qdk.Stim.Compiler.NegatedTarget

              x target cannot be negated in instruction: CX
               ,-[2:4]
             1 | M 0
             2 | CX !rec[-1] 1
               :    ^^^^^^^^
               `----
        "#]],
    );
}

#[test]
fn cx_with_rec_control_out_of_bounds_yields_error() {
    let source = "CX rec[-1] 1";
    check(
        source,
        &expect![[r#"
            Qdk.Stim.Compiler.MeasurementRecordOutOfBounds

              x measurement record is out of bounds
               ,----
             1 | CX rec[-1] 1
               :    ^^^^^^^
               `----
        "#]],
    );
}

#[test]
fn cx_with_two_rec_targets_yields_error() {
    let source = indoc! {"
        M 0
        M 1
        CX rec[-1] rec[-2]
    "};
    check(
        source,
        &expect![[r#"
            Qdk.Stim.Compiler.MeasurementRecordWithoutQubit

              x controlled instruction CX requires a qubit target, but both targets are
              | measurement records
               ,-[3:4]
             2 | M 1
             3 | CX rec[-1] rec[-2]
               :    ^^^^^^^^^^^^^^^
               `----
        "#]],
    );
}

#[test]
fn cx_with_odd_targets_including_rec_yields_error() {
    let source = indoc! {"
        M 0
        CX rec[-1]
    "};
    check(
        source,
        &expect![[r#"
            Qdk.Stim.Compiler.OddTargetCount

              x instruction CX requires an even number of targets
               ,-[2:1]
             1 | M 0
             2 | CX rec[-1]
               : ^^^^^^^^^^
               `----
        "#]],
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
            body:
                call void @__quantum__qis__m__body(ptr inttoptr (i64 0 to ptr), ptr inttoptr (i64 0 to ptr))
                call void @classical_control_cy(ptr inttoptr (i64 0 to ptr), ptr inttoptr (i64 1 to ptr))

            definitions:
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

            declarations:
              declare i1 @__quantum__rt__read_result(ptr)
              declare void @__quantum__qis__m__body(ptr, ptr)
              declare void @__quantum__qis__y__body(ptr)

            required_num_qubits: 2
            required_num_results: 1"#]],
    );
}

#[test]
fn zcy_with_rec_control_yields_expected_qir() {
    let source = indoc! {"
        M 0
        ZCY rec[-1] 1
    "};
    check(
        source,
        &expect![[r#"
            body:
                call void @__quantum__qis__m__body(ptr inttoptr (i64 0 to ptr), ptr inttoptr (i64 0 to ptr))
                call void @classical_control_cy(ptr inttoptr (i64 0 to ptr), ptr inttoptr (i64 1 to ptr))

            definitions:
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

            declarations:
              declare i1 @__quantum__rt__read_result(ptr)
              declare void @__quantum__qis__m__body(ptr, ptr)
              declare void @__quantum__qis__y__body(ptr)

            required_num_qubits: 2
            required_num_results: 1"#]],
    );
}

#[test]
fn cy_with_rec_on_second_target_yields_error() {
    let source = indoc! {"
        M 0
        CY 0 rec[-1]
    "};
    check(
        source,
        &expect![[r#"
            Qdk.Stim.Compiler.MisplacedMeasurementRecord

              x measurement record target in an unsupported position in instruction: CY
               ,-[2:6]
             1 | M 0
             2 | CY 0 rec[-1]
               :      ^^^^^^^
               `----
        "#]],
    );
}

#[test]
fn cy_with_negated_rec_control_yields_error() {
    let source = indoc! {"
        M 0
        CY !rec[-1] 1
    "};
    check(
        source,
        &expect![[r#"
            Qdk.Stim.Compiler.NegatedTarget

              x target cannot be negated in instruction: CY
               ,-[2:4]
             1 | M 0
             2 | CY !rec[-1] 1
               :    ^^^^^^^^
               `----
        "#]],
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
            body:
                call void @__quantum__qis__m__body(ptr inttoptr (i64 0 to ptr), ptr inttoptr (i64 0 to ptr))
                call void @classical_control_cz(ptr inttoptr (i64 0 to ptr), ptr inttoptr (i64 1 to ptr))

            definitions:
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

            declarations:
              declare i1 @__quantum__rt__read_result(ptr)
              declare void @__quantum__qis__m__body(ptr, ptr)
              declare void @__quantum__qis__z__body(ptr)

            required_num_qubits: 2
            required_num_results: 1"#]],
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
            body:
                call void @__quantum__qis__m__body(ptr inttoptr (i64 0 to ptr), ptr inttoptr (i64 0 to ptr))
                call void @classical_control_cz(ptr inttoptr (i64 0 to ptr), ptr inttoptr (i64 0 to ptr))

            definitions:
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

            declarations:
              declare i1 @__quantum__rt__read_result(ptr)
              declare void @__quantum__qis__m__body(ptr, ptr)
              declare void @__quantum__qis__z__body(ptr)

            required_num_qubits: 1
            required_num_results: 1"#]],
    );
}

#[test]
fn zcz_with_rec_on_first_target_yields_expected_qir() {
    let source = indoc! {"
        M 0
        ZCZ rec[-1] 1
    "};
    check(
        source,
        &expect![[r#"
            body:
                call void @__quantum__qis__m__body(ptr inttoptr (i64 0 to ptr), ptr inttoptr (i64 0 to ptr))
                call void @classical_control_cz(ptr inttoptr (i64 0 to ptr), ptr inttoptr (i64 1 to ptr))

            definitions:
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

            declarations:
              declare i1 @__quantum__rt__read_result(ptr)
              declare void @__quantum__qis__m__body(ptr, ptr)
              declare void @__quantum__qis__z__body(ptr)

            required_num_qubits: 2
            required_num_results: 1"#]],
    );
}

#[test]
fn zcz_with_rec_on_second_target_yields_expected_qir() {
    let source = indoc! {"
        M 0
        ZCZ 0 rec[-1]
    "};
    check(
        source,
        &expect![[r#"
            body:
                call void @__quantum__qis__m__body(ptr inttoptr (i64 0 to ptr), ptr inttoptr (i64 0 to ptr))
                call void @classical_control_cz(ptr inttoptr (i64 0 to ptr), ptr inttoptr (i64 0 to ptr))

            definitions:
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

            declarations:
              declare i1 @__quantum__rt__read_result(ptr)
              declare void @__quantum__qis__m__body(ptr, ptr)
              declare void @__quantum__qis__z__body(ptr)

            required_num_qubits: 1
            required_num_results: 1"#]],
    );
}

#[test]
fn cz_with_two_rec_targets_yields_error() {
    let source = indoc! {"
        M 0
        M 1
        CZ rec[-1] rec[-2]
    "};
    check(
        source,
        &expect![[r#"
            Qdk.Stim.Compiler.MeasurementRecordWithoutQubit

              x controlled instruction CZ requires a qubit target, but both targets are
              | measurement records
               ,-[3:4]
             2 | M 1
             3 | CZ rec[-1] rec[-2]
               :    ^^^^^^^^^^^^^^^
               `----
        "#]],
    );
}

#[test]
fn cz_with_negated_rec_on_first_target_yields_error() {
    let source = indoc! {"
        M 0
        CZ !rec[-1] 1
    "};
    check(
        source,
        &expect![[r#"
            Qdk.Stim.Compiler.NegatedTarget

              x target cannot be negated in instruction: CZ
               ,-[2:4]
             1 | M 0
             2 | CZ !rec[-1] 1
               :    ^^^^^^^^
               `----
        "#]],
    );
}

#[test]
fn cz_with_negated_rec_on_second_target_yields_error() {
    let source = indoc! {"
        M 0
        CZ 0 !rec[-1]
    "};
    check(
        source,
        &expect![[r#"
            Qdk.Stim.Compiler.NegatedTarget

              x target cannot be negated in instruction: CZ
               ,-[2:6]
             1 | M 0
             2 | CZ 0 !rec[-1]
               :      ^^^^^^^^
               `----
        "#]],
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
            body:
                call void @__quantum__qis__m__body(ptr inttoptr (i64 0 to ptr), ptr inttoptr (i64 0 to ptr))
                call void @classical_control_cx(ptr inttoptr (i64 0 to ptr), ptr inttoptr (i64 1 to ptr))

            definitions:
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

            declarations:
              declare i1 @__quantum__rt__read_result(ptr)
              declare void @__quantum__qis__m__body(ptr, ptr)
              declare void @__quantum__qis__x__body(ptr)

            required_num_qubits: 2
            required_num_results: 1"#]],
    );
}

#[test]
fn xcz_with_rec_on_first_target_yields_error() {
    let source = indoc! {"
        M 0
        XCZ rec[-1] 1
    "};
    check(
        source,
        &expect![[r#"
            Qdk.Stim.Compiler.MisplacedMeasurementRecord

              x measurement record target in an unsupported position in instruction: XCZ
               ,-[2:5]
             1 | M 0
             2 | XCZ rec[-1] 1
               :     ^^^^^^^
               `----
        "#]],
    );
}

#[test]
fn xcz_with_negated_rec_on_second_target_yields_error() {
    let source = indoc! {"
        M 0
        XCZ 1 !rec[-1]
    "};
    check(
        source,
        &expect![[r#"
            Qdk.Stim.Compiler.NegatedTarget

              x target cannot be negated in instruction: XCZ
               ,-[2:7]
             1 | M 0
             2 | XCZ 1 !rec[-1]
               :       ^^^^^^^^
               `----
        "#]],
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
            body:
                call void @__quantum__qis__m__body(ptr inttoptr (i64 0 to ptr), ptr inttoptr (i64 0 to ptr))
                call void @classical_control_cy(ptr inttoptr (i64 0 to ptr), ptr inttoptr (i64 1 to ptr))

            definitions:
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

            declarations:
              declare i1 @__quantum__rt__read_result(ptr)
              declare void @__quantum__qis__m__body(ptr, ptr)
              declare void @__quantum__qis__y__body(ptr)

            required_num_qubits: 2
            required_num_results: 1"#]],
    );
}

#[test]
fn ycz_with_rec_on_first_target_yields_error() {
    let source = indoc! {"
        M 0
        YCZ rec[-1] 1
    "};
    check(
        source,
        &expect![[r#"
            Qdk.Stim.Compiler.MisplacedMeasurementRecord

              x measurement record target in an unsupported position in instruction: YCZ
               ,-[2:5]
             1 | M 0
             2 | YCZ rec[-1] 1
               :     ^^^^^^^
               `----
        "#]],
    );
}

#[test]
fn ycz_with_negated_rec_on_second_target_yields_error() {
    let source = indoc! {"
        M 0
        YCZ 1 !rec[-1]
    "};
    check(
        source,
        &expect![[r#"
            Qdk.Stim.Compiler.NegatedTarget

              x target cannot be negated in instruction: YCZ
               ,-[2:7]
             1 | M 0
             2 | YCZ 1 !rec[-1]
               :       ^^^^^^^^
               `----
        "#]],
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
            body:
                call void @__quantum__qis__m__body(ptr inttoptr (i64 0 to ptr), ptr inttoptr (i64 0 to ptr))
                br label %select_0
              select_0:
                call void @classical_control_cx(ptr inttoptr (i64 0 to ptr), ptr inttoptr (i64 1 to ptr))

            definitions:
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

            declarations:
              declare i1 @__quantum__rt__read_result(ptr)
              declare void @__quantum__qis__m__body(ptr, ptr)
              declare void @__quantum__qis__x__body(ptr)

            required_num_qubits: 2
            required_num_results: 1"#]],
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
            body:
                br label %select_0
              select_0:
                call void @__quantum__qis__m__body(ptr inttoptr (i64 0 to ptr), ptr inttoptr (i64 0 to ptr))
                call void @classical_control_cx(ptr inttoptr (i64 0 to ptr), ptr inttoptr (i64 1 to ptr))

            definitions:
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

            declarations:
              declare i1 @__quantum__rt__read_result(ptr)
              declare void @__quantum__qis__m__body(ptr, ptr)
              declare void @__quantum__qis__x__body(ptr)

            required_num_qubits: 2
            required_num_results: 1"#]],
    );
}
