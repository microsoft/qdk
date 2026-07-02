// Copyright (c) Microsoft Corporation.
// Licensed under the MIT License.

use crate::qir::tests::check;
use expect_test::expect;

#[test]
fn cx_with_rec_control_yields_expected_qir() {
    let source = "M 0\nCX rec[-1] 1";
    check(
        source,
        &expect![[r#"
        define i64 @ENTRYPOINT__main() #0 {
          call void @__quantum__rt__initialize(ptr null)
          call void @__quantum__qis__m__body(ptr inttoptr (i64 0 to ptr), ptr inttoptr (i64 0 to ptr))
          %r_0 = call i1 @__quantum__rt__read_result(ptr inttoptr (i64 0 to ptr))
          br i1 %r_0, label %apply_controlled_0, label %continue_controlled_0
        apply_controlled_0:
          call void @__quantum__qis__x__body(ptr inttoptr (i64 1 to ptr))
          br label %continue_controlled_0
        continue_controlled_0:
          call void @__quantum__rt__array_record_output(i64 1, ptr null)
          call void @__quantum__rt__result_record_output(ptr inttoptr (i64 0 to ptr), ptr null)
          ret i64 0
        }

        declare void @__quantum__rt__array_record_output(i64, ptr)
        declare void @__quantum__rt__result_record_output(ptr, ptr)
        declare void @__quantum__qis__x__body(ptr)
        declare i1 @__quantum__rt__read_result(ptr)
        declare void @__quantum__rt__initialize(ptr)
        declare void @__quantum__qis__m__body(ptr, ptr)

        attributes #0 = { "entry_point" "output_labeling_schema" "qir_profiles"="adaptive_profile" "required_num_qubits"="2" "required_num_results"="1" }
        attributes #1 = { "irreversible" }

        ; module flags

        !llvm.module.flags = !{!0, !1, !2, !3, !4, !5, !6, !7}

        !0 = !{i32 1, !"qir_major_version", i32 2}
        !1 = !{i32 7, !"qir_minor_version", i32 1}
        !2 = !{i32 1, !"dynamic_qubit_management", i1 false}
        !3 = !{i32 1, !"dynamic_result_management", i1 false}
        !4 = !{i32 5, !"int_computations", !{!"i64"}}
        !5 = !{i32 5, !"float_computations", !{!"double"}}
        !6 = !{i32 7, !"backwards_branching", i2 3}
        !7 = !{i32 1, !"arrays", i1 true}
    "#]],
    );
}

#[test]
fn cnot_with_rec_control_yields_expected_qir() {
    let source = "M 0\nCNOT rec[-1] 1";
    check(
        source,
        &expect![[r#"
        define i64 @ENTRYPOINT__main() #0 {
          call void @__quantum__rt__initialize(ptr null)
          call void @__quantum__qis__m__body(ptr inttoptr (i64 0 to ptr), ptr inttoptr (i64 0 to ptr))
          %r_0 = call i1 @__quantum__rt__read_result(ptr inttoptr (i64 0 to ptr))
          br i1 %r_0, label %apply_controlled_0, label %continue_controlled_0
        apply_controlled_0:
          call void @__quantum__qis__x__body(ptr inttoptr (i64 1 to ptr))
          br label %continue_controlled_0
        continue_controlled_0:
          call void @__quantum__rt__array_record_output(i64 1, ptr null)
          call void @__quantum__rt__result_record_output(ptr inttoptr (i64 0 to ptr), ptr null)
          ret i64 0
        }

        declare void @__quantum__rt__array_record_output(i64, ptr)
        declare void @__quantum__rt__result_record_output(ptr, ptr)
        declare void @__quantum__qis__x__body(ptr)
        declare i1 @__quantum__rt__read_result(ptr)
        declare void @__quantum__rt__initialize(ptr)
        declare void @__quantum__qis__m__body(ptr, ptr)

        attributes #0 = { "entry_point" "output_labeling_schema" "qir_profiles"="adaptive_profile" "required_num_qubits"="2" "required_num_results"="1" }
        attributes #1 = { "irreversible" }

        ; module flags

        !llvm.module.flags = !{!0, !1, !2, !3, !4, !5, !6, !7}

        !0 = !{i32 1, !"qir_major_version", i32 2}
        !1 = !{i32 7, !"qir_minor_version", i32 1}
        !2 = !{i32 1, !"dynamic_qubit_management", i1 false}
        !3 = !{i32 1, !"dynamic_result_management", i1 false}
        !4 = !{i32 5, !"int_computations", !{!"i64"}}
        !5 = !{i32 5, !"float_computations", !{!"double"}}
        !6 = !{i32 7, !"backwards_branching", i2 3}
        !7 = !{i32 1, !"arrays", i1 true}
    "#]],
    );
}

#[test]
fn zcx_with_rec_control_yields_expected_qir() {
    let source = "M 0\nZCX rec[-1] 1";
    check(
        source,
        &expect![[r#"
        define i64 @ENTRYPOINT__main() #0 {
          call void @__quantum__rt__initialize(ptr null)
          call void @__quantum__qis__m__body(ptr inttoptr (i64 0 to ptr), ptr inttoptr (i64 0 to ptr))
          %r_0 = call i1 @__quantum__rt__read_result(ptr inttoptr (i64 0 to ptr))
          br i1 %r_0, label %apply_controlled_0, label %continue_controlled_0
        apply_controlled_0:
          call void @__quantum__qis__x__body(ptr inttoptr (i64 1 to ptr))
          br label %continue_controlled_0
        continue_controlled_0:
          call void @__quantum__rt__array_record_output(i64 1, ptr null)
          call void @__quantum__rt__result_record_output(ptr inttoptr (i64 0 to ptr), ptr null)
          ret i64 0
        }

        declare void @__quantum__rt__array_record_output(i64, ptr)
        declare void @__quantum__rt__result_record_output(ptr, ptr)
        declare void @__quantum__qis__x__body(ptr)
        declare i1 @__quantum__rt__read_result(ptr)
        declare void @__quantum__rt__initialize(ptr)
        declare void @__quantum__qis__m__body(ptr, ptr)

        attributes #0 = { "entry_point" "output_labeling_schema" "qir_profiles"="adaptive_profile" "required_num_qubits"="2" "required_num_results"="1" }
        attributes #1 = { "irreversible" }

        ; module flags

        !llvm.module.flags = !{!0, !1, !2, !3, !4, !5, !6, !7}

        !0 = !{i32 1, !"qir_major_version", i32 2}
        !1 = !{i32 7, !"qir_minor_version", i32 1}
        !2 = !{i32 1, !"dynamic_qubit_management", i1 false}
        !3 = !{i32 1, !"dynamic_result_management", i1 false}
        !4 = !{i32 5, !"int_computations", !{!"i64"}}
        !5 = !{i32 5, !"float_computations", !{!"double"}}
        !6 = !{i32 7, !"backwards_branching", i2 3}
        !7 = !{i32 1, !"arrays", i1 true}
    "#]],
    );
}

#[test]
fn cx_with_older_rec_control_yields_expected_qir() {
    let source = "M 0\nM 1\nCX rec[-2] 2";
    check(
        source,
        &expect![[r#"
        define i64 @ENTRYPOINT__main() #0 {
          call void @__quantum__rt__initialize(ptr null)
          call void @__quantum__qis__m__body(ptr inttoptr (i64 0 to ptr), ptr inttoptr (i64 0 to ptr))
          call void @__quantum__qis__m__body(ptr inttoptr (i64 1 to ptr), ptr inttoptr (i64 1 to ptr))
          %r_0 = call i1 @__quantum__rt__read_result(ptr inttoptr (i64 0 to ptr))
          br i1 %r_0, label %apply_controlled_0, label %continue_controlled_0
        apply_controlled_0:
          call void @__quantum__qis__x__body(ptr inttoptr (i64 2 to ptr))
          br label %continue_controlled_0
        continue_controlled_0:
          call void @__quantum__rt__array_record_output(i64 2, ptr null)
          call void @__quantum__rt__result_record_output(ptr inttoptr (i64 0 to ptr), ptr null)
          call void @__quantum__rt__result_record_output(ptr inttoptr (i64 1 to ptr), ptr null)
          ret i64 0
        }

        declare void @__quantum__rt__array_record_output(i64, ptr)
        declare void @__quantum__rt__result_record_output(ptr, ptr)
        declare void @__quantum__qis__x__body(ptr)
        declare i1 @__quantum__rt__read_result(ptr)
        declare void @__quantum__rt__initialize(ptr)
        declare void @__quantum__qis__m__body(ptr, ptr)

        attributes #0 = { "entry_point" "output_labeling_schema" "qir_profiles"="adaptive_profile" "required_num_qubits"="3" "required_num_results"="2" }
        attributes #1 = { "irreversible" }

        ; module flags

        !llvm.module.flags = !{!0, !1, !2, !3, !4, !5, !6, !7}

        !0 = !{i32 1, !"qir_major_version", i32 2}
        !1 = !{i32 7, !"qir_minor_version", i32 1}
        !2 = !{i32 1, !"dynamic_qubit_management", i1 false}
        !3 = !{i32 1, !"dynamic_result_management", i1 false}
        !4 = !{i32 5, !"int_computations", !{!"i64"}}
        !5 = !{i32 5, !"float_computations", !{!"double"}}
        !6 = !{i32 7, !"backwards_branching", i2 3}
        !7 = !{i32 1, !"arrays", i1 true}
    "#]],
    );
}

#[test]
fn cx_with_mixed_quantum_and_classical_pairs_yields_expected_qir() {
    let source = "M 0\nCX rec[-1] 1 2 3";
    check(
        source,
        &expect![[r#"
        define i64 @ENTRYPOINT__main() #0 {
          call void @__quantum__rt__initialize(ptr null)
          call void @__quantum__qis__m__body(ptr inttoptr (i64 0 to ptr), ptr inttoptr (i64 0 to ptr))
          %r_0 = call i1 @__quantum__rt__read_result(ptr inttoptr (i64 0 to ptr))
          br i1 %r_0, label %apply_controlled_0, label %continue_controlled_0
        apply_controlled_0:
          call void @__quantum__qis__x__body(ptr inttoptr (i64 1 to ptr))
          br label %continue_controlled_0
        continue_controlled_0:
          call void @__quantum__qis__cx__body(ptr inttoptr (i64 2 to ptr), ptr inttoptr (i64 3 to ptr))
          call void @__quantum__rt__array_record_output(i64 1, ptr null)
          call void @__quantum__rt__result_record_output(ptr inttoptr (i64 0 to ptr), ptr null)
          ret i64 0
        }

        declare void @__quantum__qis__cx__body(ptr, ptr)
        declare void @__quantum__rt__array_record_output(i64, ptr)
        declare void @__quantum__rt__result_record_output(ptr, ptr)
        declare void @__quantum__qis__x__body(ptr)
        declare i1 @__quantum__rt__read_result(ptr)
        declare void @__quantum__rt__initialize(ptr)
        declare void @__quantum__qis__m__body(ptr, ptr)

        attributes #0 = { "entry_point" "output_labeling_schema" "qir_profiles"="adaptive_profile" "required_num_qubits"="4" "required_num_results"="1" }
        attributes #1 = { "irreversible" }

        ; module flags

        !llvm.module.flags = !{!0, !1, !2, !3, !4, !5, !6, !7}

        !0 = !{i32 1, !"qir_major_version", i32 2}
        !1 = !{i32 7, !"qir_minor_version", i32 1}
        !2 = !{i32 1, !"dynamic_qubit_management", i1 false}
        !3 = !{i32 1, !"dynamic_result_management", i1 false}
        !4 = !{i32 5, !"int_computations", !{!"i64"}}
        !5 = !{i32 5, !"float_computations", !{!"double"}}
        !6 = !{i32 7, !"backwards_branching", i2 3}
        !7 = !{i32 1, !"arrays", i1 true}
    "#]],
    );
}

#[test]
fn cx_with_multiple_classical_pairs_yields_expected_qir() {
    let source = "M 0\nM 1\nCX rec[-1] 2 rec[-2] 3";
    check(
        source,
        &expect![[r#"
        define i64 @ENTRYPOINT__main() #0 {
          call void @__quantum__rt__initialize(ptr null)
          call void @__quantum__qis__m__body(ptr inttoptr (i64 0 to ptr), ptr inttoptr (i64 0 to ptr))
          call void @__quantum__qis__m__body(ptr inttoptr (i64 1 to ptr), ptr inttoptr (i64 1 to ptr))
          %r_0 = call i1 @__quantum__rt__read_result(ptr inttoptr (i64 1 to ptr))
          br i1 %r_0, label %apply_controlled_0, label %continue_controlled_0
        apply_controlled_0:
          call void @__quantum__qis__x__body(ptr inttoptr (i64 2 to ptr))
          br label %continue_controlled_0
        continue_controlled_0:
          %r_1 = call i1 @__quantum__rt__read_result(ptr inttoptr (i64 0 to ptr))
          br i1 %r_1, label %apply_controlled_1, label %continue_controlled_1
        apply_controlled_1:
          call void @__quantum__qis__x__body(ptr inttoptr (i64 3 to ptr))
          br label %continue_controlled_1
        continue_controlled_1:
          call void @__quantum__rt__array_record_output(i64 2, ptr null)
          call void @__quantum__rt__result_record_output(ptr inttoptr (i64 0 to ptr), ptr null)
          call void @__quantum__rt__result_record_output(ptr inttoptr (i64 1 to ptr), ptr null)
          ret i64 0
        }

        declare void @__quantum__rt__array_record_output(i64, ptr)
        declare void @__quantum__rt__result_record_output(ptr, ptr)
        declare void @__quantum__qis__x__body(ptr)
        declare i1 @__quantum__rt__read_result(ptr)
        declare void @__quantum__rt__initialize(ptr)
        declare void @__quantum__qis__m__body(ptr, ptr)

        attributes #0 = { "entry_point" "output_labeling_schema" "qir_profiles"="adaptive_profile" "required_num_qubits"="4" "required_num_results"="2" }
        attributes #1 = { "irreversible" }

        ; module flags

        !llvm.module.flags = !{!0, !1, !2, !3, !4, !5, !6, !7}

        !0 = !{i32 1, !"qir_major_version", i32 2}
        !1 = !{i32 7, !"qir_minor_version", i32 1}
        !2 = !{i32 1, !"dynamic_qubit_management", i1 false}
        !3 = !{i32 1, !"dynamic_result_management", i1 false}
        !4 = !{i32 5, !"int_computations", !{!"i64"}}
        !5 = !{i32 5, !"float_computations", !{!"double"}}
        !6 = !{i32 7, !"backwards_branching", i2 3}
        !7 = !{i32 1, !"arrays", i1 true}
    "#]],
    );
}

#[test]
fn cx_with_rec_on_second_target_yields_error() {
    let source = "M 0\nCX 0 rec[-1]";
    check(
        source,
        &expect![[r#"
        Stim.MisplacedMeasurementRecord

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
    let source = "M 0\nCX !rec[-1] 1";
    check(
        source,
        &expect![[r#"
        Stim.NegatedMeasurementRecord

          x measurement record control cannot be negated in instruction: CX
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
        Stim.MeasurementRecordOutOfBounds

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
    let source = "M 0\nM 1\nCX rec[-1] rec[-2]";
    check(
        source,
        &expect![[r#"
        Stim.MeasurementRecordWithoutQubit

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
    let source = "M 0\nCX rec[-1]";
    check(
        source,
        &expect![[r#"
        Stim.OddTargetCount

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
    let source = "M 0\nCY rec[-1] 1";
    check(
        source,
        &expect![[r#"
        define i64 @ENTRYPOINT__main() #0 {
          call void @__quantum__rt__initialize(ptr null)
          call void @__quantum__qis__m__body(ptr inttoptr (i64 0 to ptr), ptr inttoptr (i64 0 to ptr))
          %r_0 = call i1 @__quantum__rt__read_result(ptr inttoptr (i64 0 to ptr))
          br i1 %r_0, label %apply_controlled_0, label %continue_controlled_0
        apply_controlled_0:
          call void @__quantum__qis__y__body(ptr inttoptr (i64 1 to ptr))
          br label %continue_controlled_0
        continue_controlled_0:
          call void @__quantum__rt__array_record_output(i64 1, ptr null)
          call void @__quantum__rt__result_record_output(ptr inttoptr (i64 0 to ptr), ptr null)
          ret i64 0
        }

        declare void @__quantum__qis__y__body(ptr)
        declare void @__quantum__rt__result_record_output(ptr, ptr)
        declare void @__quantum__rt__array_record_output(i64, ptr)
        declare i1 @__quantum__rt__read_result(ptr)
        declare void @__quantum__rt__initialize(ptr)
        declare void @__quantum__qis__m__body(ptr, ptr)

        attributes #0 = { "entry_point" "output_labeling_schema" "qir_profiles"="adaptive_profile" "required_num_qubits"="2" "required_num_results"="1" }
        attributes #1 = { "irreversible" }

        ; module flags

        !llvm.module.flags = !{!0, !1, !2, !3, !4, !5, !6, !7}

        !0 = !{i32 1, !"qir_major_version", i32 2}
        !1 = !{i32 7, !"qir_minor_version", i32 1}
        !2 = !{i32 1, !"dynamic_qubit_management", i1 false}
        !3 = !{i32 1, !"dynamic_result_management", i1 false}
        !4 = !{i32 5, !"int_computations", !{!"i64"}}
        !5 = !{i32 5, !"float_computations", !{!"double"}}
        !6 = !{i32 7, !"backwards_branching", i2 3}
        !7 = !{i32 1, !"arrays", i1 true}
    "#]],
    );
}

#[test]
fn zcy_with_rec_control_yields_expected_qir() {
    let source = "M 0\nZCY rec[-1] 1";
    check(
        source,
        &expect![[r#"
        define i64 @ENTRYPOINT__main() #0 {
          call void @__quantum__rt__initialize(ptr null)
          call void @__quantum__qis__m__body(ptr inttoptr (i64 0 to ptr), ptr inttoptr (i64 0 to ptr))
          %r_0 = call i1 @__quantum__rt__read_result(ptr inttoptr (i64 0 to ptr))
          br i1 %r_0, label %apply_controlled_0, label %continue_controlled_0
        apply_controlled_0:
          call void @__quantum__qis__y__body(ptr inttoptr (i64 1 to ptr))
          br label %continue_controlled_0
        continue_controlled_0:
          call void @__quantum__rt__array_record_output(i64 1, ptr null)
          call void @__quantum__rt__result_record_output(ptr inttoptr (i64 0 to ptr), ptr null)
          ret i64 0
        }

        declare void @__quantum__qis__y__body(ptr)
        declare void @__quantum__rt__result_record_output(ptr, ptr)
        declare void @__quantum__rt__array_record_output(i64, ptr)
        declare i1 @__quantum__rt__read_result(ptr)
        declare void @__quantum__rt__initialize(ptr)
        declare void @__quantum__qis__m__body(ptr, ptr)

        attributes #0 = { "entry_point" "output_labeling_schema" "qir_profiles"="adaptive_profile" "required_num_qubits"="2" "required_num_results"="1" }
        attributes #1 = { "irreversible" }

        ; module flags

        !llvm.module.flags = !{!0, !1, !2, !3, !4, !5, !6, !7}

        !0 = !{i32 1, !"qir_major_version", i32 2}
        !1 = !{i32 7, !"qir_minor_version", i32 1}
        !2 = !{i32 1, !"dynamic_qubit_management", i1 false}
        !3 = !{i32 1, !"dynamic_result_management", i1 false}
        !4 = !{i32 5, !"int_computations", !{!"i64"}}
        !5 = !{i32 5, !"float_computations", !{!"double"}}
        !6 = !{i32 7, !"backwards_branching", i2 3}
        !7 = !{i32 1, !"arrays", i1 true}
    "#]],
    );
}

#[test]
fn cy_with_rec_on_second_target_yields_error() {
    let source = "M 0\nCY 0 rec[-1]";
    check(
        source,
        &expect![[r#"
        Stim.MisplacedMeasurementRecord

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
    let source = "M 0\nCY !rec[-1] 1";
    check(
        source,
        &expect![[r#"
        Stim.NegatedMeasurementRecord

          x measurement record control cannot be negated in instruction: CY
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
    let source = "M 0\nCZ rec[-1] 1";
    check(
        source,
        &expect![[r#"
        define i64 @ENTRYPOINT__main() #0 {
          call void @__quantum__rt__initialize(ptr null)
          call void @__quantum__qis__m__body(ptr inttoptr (i64 0 to ptr), ptr inttoptr (i64 0 to ptr))
          %r_0 = call i1 @__quantum__rt__read_result(ptr inttoptr (i64 0 to ptr))
          br i1 %r_0, label %apply_controlled_0, label %continue_controlled_0
        apply_controlled_0:
          call void @__quantum__qis__z__body(ptr inttoptr (i64 1 to ptr))
          br label %continue_controlled_0
        continue_controlled_0:
          call void @__quantum__rt__array_record_output(i64 1, ptr null)
          call void @__quantum__rt__result_record_output(ptr inttoptr (i64 0 to ptr), ptr null)
          ret i64 0
        }

        declare void @__quantum__rt__array_record_output(i64, ptr)
        declare void @__quantum__rt__result_record_output(ptr, ptr)
        declare void @__quantum__qis__z__body(ptr)
        declare i1 @__quantum__rt__read_result(ptr)
        declare void @__quantum__rt__initialize(ptr)
        declare void @__quantum__qis__m__body(ptr, ptr)

        attributes #0 = { "entry_point" "output_labeling_schema" "qir_profiles"="adaptive_profile" "required_num_qubits"="2" "required_num_results"="1" }
        attributes #1 = { "irreversible" }

        ; module flags

        !llvm.module.flags = !{!0, !1, !2, !3, !4, !5, !6, !7}

        !0 = !{i32 1, !"qir_major_version", i32 2}
        !1 = !{i32 7, !"qir_minor_version", i32 1}
        !2 = !{i32 1, !"dynamic_qubit_management", i1 false}
        !3 = !{i32 1, !"dynamic_result_management", i1 false}
        !4 = !{i32 5, !"int_computations", !{!"i64"}}
        !5 = !{i32 5, !"float_computations", !{!"double"}}
        !6 = !{i32 7, !"backwards_branching", i2 3}
        !7 = !{i32 1, !"arrays", i1 true}
    "#]],
    );
}

#[test]
fn cz_with_rec_on_second_target_yields_expected_qir() {
    let source = "M 0\nCZ 0 rec[-1]";
    check(
        source,
        &expect![[r#"
        define i64 @ENTRYPOINT__main() #0 {
          call void @__quantum__rt__initialize(ptr null)
          call void @__quantum__qis__m__body(ptr inttoptr (i64 0 to ptr), ptr inttoptr (i64 0 to ptr))
          %r_0 = call i1 @__quantum__rt__read_result(ptr inttoptr (i64 0 to ptr))
          br i1 %r_0, label %apply_controlled_0, label %continue_controlled_0
        apply_controlled_0:
          call void @__quantum__qis__z__body(ptr inttoptr (i64 0 to ptr))
          br label %continue_controlled_0
        continue_controlled_0:
          call void @__quantum__rt__array_record_output(i64 1, ptr null)
          call void @__quantum__rt__result_record_output(ptr inttoptr (i64 0 to ptr), ptr null)
          ret i64 0
        }

        declare void @__quantum__rt__array_record_output(i64, ptr)
        declare void @__quantum__rt__result_record_output(ptr, ptr)
        declare void @__quantum__qis__z__body(ptr)
        declare i1 @__quantum__rt__read_result(ptr)
        declare void @__quantum__rt__initialize(ptr)
        declare void @__quantum__qis__m__body(ptr, ptr)

        attributes #0 = { "entry_point" "output_labeling_schema" "qir_profiles"="adaptive_profile" "required_num_qubits"="1" "required_num_results"="1" }
        attributes #1 = { "irreversible" }

        ; module flags

        !llvm.module.flags = !{!0, !1, !2, !3, !4, !5, !6, !7}

        !0 = !{i32 1, !"qir_major_version", i32 2}
        !1 = !{i32 7, !"qir_minor_version", i32 1}
        !2 = !{i32 1, !"dynamic_qubit_management", i1 false}
        !3 = !{i32 1, !"dynamic_result_management", i1 false}
        !4 = !{i32 5, !"int_computations", !{!"i64"}}
        !5 = !{i32 5, !"float_computations", !{!"double"}}
        !6 = !{i32 7, !"backwards_branching", i2 3}
        !7 = !{i32 1, !"arrays", i1 true}
    "#]],
    );
}

#[test]
fn zcz_with_rec_on_first_target_yields_expected_qir() {
    let source = "M 0\nZCZ rec[-1] 1";
    check(
        source,
        &expect![[r#"
        define i64 @ENTRYPOINT__main() #0 {
          call void @__quantum__rt__initialize(ptr null)
          call void @__quantum__qis__m__body(ptr inttoptr (i64 0 to ptr), ptr inttoptr (i64 0 to ptr))
          %r_0 = call i1 @__quantum__rt__read_result(ptr inttoptr (i64 0 to ptr))
          br i1 %r_0, label %apply_controlled_0, label %continue_controlled_0
        apply_controlled_0:
          call void @__quantum__qis__z__body(ptr inttoptr (i64 1 to ptr))
          br label %continue_controlled_0
        continue_controlled_0:
          call void @__quantum__rt__array_record_output(i64 1, ptr null)
          call void @__quantum__rt__result_record_output(ptr inttoptr (i64 0 to ptr), ptr null)
          ret i64 0
        }

        declare void @__quantum__rt__array_record_output(i64, ptr)
        declare void @__quantum__rt__result_record_output(ptr, ptr)
        declare void @__quantum__qis__z__body(ptr)
        declare i1 @__quantum__rt__read_result(ptr)
        declare void @__quantum__rt__initialize(ptr)
        declare void @__quantum__qis__m__body(ptr, ptr)

        attributes #0 = { "entry_point" "output_labeling_schema" "qir_profiles"="adaptive_profile" "required_num_qubits"="2" "required_num_results"="1" }
        attributes #1 = { "irreversible" }

        ; module flags

        !llvm.module.flags = !{!0, !1, !2, !3, !4, !5, !6, !7}

        !0 = !{i32 1, !"qir_major_version", i32 2}
        !1 = !{i32 7, !"qir_minor_version", i32 1}
        !2 = !{i32 1, !"dynamic_qubit_management", i1 false}
        !3 = !{i32 1, !"dynamic_result_management", i1 false}
        !4 = !{i32 5, !"int_computations", !{!"i64"}}
        !5 = !{i32 5, !"float_computations", !{!"double"}}
        !6 = !{i32 7, !"backwards_branching", i2 3}
        !7 = !{i32 1, !"arrays", i1 true}
    "#]],
    );
}

#[test]
fn zcz_with_rec_on_second_target_yields_expected_qir() {
    let source = "M 0\nZCZ 0 rec[-1]";
    check(
        source,
        &expect![[r#"
        define i64 @ENTRYPOINT__main() #0 {
          call void @__quantum__rt__initialize(ptr null)
          call void @__quantum__qis__m__body(ptr inttoptr (i64 0 to ptr), ptr inttoptr (i64 0 to ptr))
          %r_0 = call i1 @__quantum__rt__read_result(ptr inttoptr (i64 0 to ptr))
          br i1 %r_0, label %apply_controlled_0, label %continue_controlled_0
        apply_controlled_0:
          call void @__quantum__qis__z__body(ptr inttoptr (i64 0 to ptr))
          br label %continue_controlled_0
        continue_controlled_0:
          call void @__quantum__rt__array_record_output(i64 1, ptr null)
          call void @__quantum__rt__result_record_output(ptr inttoptr (i64 0 to ptr), ptr null)
          ret i64 0
        }

        declare void @__quantum__rt__array_record_output(i64, ptr)
        declare void @__quantum__rt__result_record_output(ptr, ptr)
        declare void @__quantum__qis__z__body(ptr)
        declare i1 @__quantum__rt__read_result(ptr)
        declare void @__quantum__rt__initialize(ptr)
        declare void @__quantum__qis__m__body(ptr, ptr)

        attributes #0 = { "entry_point" "output_labeling_schema" "qir_profiles"="adaptive_profile" "required_num_qubits"="1" "required_num_results"="1" }
        attributes #1 = { "irreversible" }

        ; module flags

        !llvm.module.flags = !{!0, !1, !2, !3, !4, !5, !6, !7}

        !0 = !{i32 1, !"qir_major_version", i32 2}
        !1 = !{i32 7, !"qir_minor_version", i32 1}
        !2 = !{i32 1, !"dynamic_qubit_management", i1 false}
        !3 = !{i32 1, !"dynamic_result_management", i1 false}
        !4 = !{i32 5, !"int_computations", !{!"i64"}}
        !5 = !{i32 5, !"float_computations", !{!"double"}}
        !6 = !{i32 7, !"backwards_branching", i2 3}
        !7 = !{i32 1, !"arrays", i1 true}
    "#]],
    );
}

#[test]
fn cz_with_two_rec_targets_yields_error() {
    let source = "M 0\nM 1\nCZ rec[-1] rec[-2]";
    check(
        source,
        &expect![[r#"
        Stim.MeasurementRecordWithoutQubit

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
    let source = "M 0\nCZ !rec[-1] 1";
    check(
        source,
        &expect![[r#"
        Stim.NegatedMeasurementRecord

          x measurement record control cannot be negated in instruction: CZ
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
    let source = "M 0\nCZ 0 !rec[-1]";
    check(
        source,
        &expect![[r#"
        Stim.NegatedMeasurementRecord

          x measurement record control cannot be negated in instruction: CZ
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
    let source = "M 0\nXCZ 1 rec[-1]";
    check(
        source,
        &expect![[r#"
        define i64 @ENTRYPOINT__main() #0 {
          call void @__quantum__rt__initialize(ptr null)
          call void @__quantum__qis__m__body(ptr inttoptr (i64 0 to ptr), ptr inttoptr (i64 0 to ptr))
          %r_0 = call i1 @__quantum__rt__read_result(ptr inttoptr (i64 0 to ptr))
          br i1 %r_0, label %apply_controlled_0, label %continue_controlled_0
        apply_controlled_0:
          call void @__quantum__qis__x__body(ptr inttoptr (i64 1 to ptr))
          br label %continue_controlled_0
        continue_controlled_0:
          call void @__quantum__rt__array_record_output(i64 1, ptr null)
          call void @__quantum__rt__result_record_output(ptr inttoptr (i64 0 to ptr), ptr null)
          ret i64 0
        }

        declare void @__quantum__rt__array_record_output(i64, ptr)
        declare void @__quantum__rt__result_record_output(ptr, ptr)
        declare void @__quantum__qis__x__body(ptr)
        declare i1 @__quantum__rt__read_result(ptr)
        declare void @__quantum__rt__initialize(ptr)
        declare void @__quantum__qis__m__body(ptr, ptr)

        attributes #0 = { "entry_point" "output_labeling_schema" "qir_profiles"="adaptive_profile" "required_num_qubits"="2" "required_num_results"="1" }
        attributes #1 = { "irreversible" }

        ; module flags

        !llvm.module.flags = !{!0, !1, !2, !3, !4, !5, !6, !7}

        !0 = !{i32 1, !"qir_major_version", i32 2}
        !1 = !{i32 7, !"qir_minor_version", i32 1}
        !2 = !{i32 1, !"dynamic_qubit_management", i1 false}
        !3 = !{i32 1, !"dynamic_result_management", i1 false}
        !4 = !{i32 5, !"int_computations", !{!"i64"}}
        !5 = !{i32 5, !"float_computations", !{!"double"}}
        !6 = !{i32 7, !"backwards_branching", i2 3}
        !7 = !{i32 1, !"arrays", i1 true}
    "#]],
    );
}

#[test]
fn xcz_with_rec_on_first_target_yields_error() {
    let source = "M 0\nXCZ rec[-1] 1";
    check(
        source,
        &expect![[r#"
        Stim.MisplacedMeasurementRecord

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
    let source = "M 0\nXCZ 1 !rec[-1]";
    check(
        source,
        &expect![[r#"
        Stim.NegatedMeasurementRecord

          x measurement record control cannot be negated in instruction: XCZ
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
    let source = "M 0\nYCZ 1 rec[-1]";
    check(
        source,
        &expect![[r#"
        define i64 @ENTRYPOINT__main() #0 {
          call void @__quantum__rt__initialize(ptr null)
          call void @__quantum__qis__m__body(ptr inttoptr (i64 0 to ptr), ptr inttoptr (i64 0 to ptr))
          %r_0 = call i1 @__quantum__rt__read_result(ptr inttoptr (i64 0 to ptr))
          br i1 %r_0, label %apply_controlled_0, label %continue_controlled_0
        apply_controlled_0:
          call void @__quantum__qis__y__body(ptr inttoptr (i64 1 to ptr))
          br label %continue_controlled_0
        continue_controlled_0:
          call void @__quantum__rt__array_record_output(i64 1, ptr null)
          call void @__quantum__rt__result_record_output(ptr inttoptr (i64 0 to ptr), ptr null)
          ret i64 0
        }

        declare void @__quantum__qis__y__body(ptr)
        declare void @__quantum__rt__result_record_output(ptr, ptr)
        declare void @__quantum__rt__array_record_output(i64, ptr)
        declare i1 @__quantum__rt__read_result(ptr)
        declare void @__quantum__rt__initialize(ptr)
        declare void @__quantum__qis__m__body(ptr, ptr)

        attributes #0 = { "entry_point" "output_labeling_schema" "qir_profiles"="adaptive_profile" "required_num_qubits"="2" "required_num_results"="1" }
        attributes #1 = { "irreversible" }

        ; module flags

        !llvm.module.flags = !{!0, !1, !2, !3, !4, !5, !6, !7}

        !0 = !{i32 1, !"qir_major_version", i32 2}
        !1 = !{i32 7, !"qir_minor_version", i32 1}
        !2 = !{i32 1, !"dynamic_qubit_management", i1 false}
        !3 = !{i32 1, !"dynamic_result_management", i1 false}
        !4 = !{i32 5, !"int_computations", !{!"i64"}}
        !5 = !{i32 5, !"float_computations", !{!"double"}}
        !6 = !{i32 7, !"backwards_branching", i2 3}
        !7 = !{i32 1, !"arrays", i1 true}
    "#]],
    );
}

#[test]
fn ycz_with_rec_on_first_target_yields_error() {
    let source = "M 0\nYCZ rec[-1] 1";
    check(
        source,
        &expect![[r#"
        Stim.MisplacedMeasurementRecord

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
    let source = "M 0\nYCZ 1 !rec[-1]";
    check(
        source,
        &expect![[r#"
        Stim.NegatedMeasurementRecord

          x measurement record control cannot be negated in instruction: YCZ
           ,-[2:7]
         1 | M 0
         2 | YCZ 1 !rec[-1]
           :       ^^^^^^^^
           `----
    "#]],
    );
}

#[test]
fn cx_with_rec_control_crossing_prepare_boundary_yields_error() {
    let source = "M 0\nPREPARE {\n    CX rec[-1] 1\n}";
    check(
        source,
        &expect![[r#"
        Stim.MeasurementRecordOutOfScope

          x measurement record refers to a measurement outside the enclosing PREPARE
          | block
           ,-[3:8]
         2 | PREPARE {
         3 |     CX rec[-1] 1
           :        ^^^^^^^
         4 | }
           `----
    "#]],
    );
}
