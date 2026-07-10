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

    check(
        source,
        &expect![[r#"
            define i64 @ENTRYPOINT__main() #0 {
              call void @__quantum__rt__initialize(ptr null)
              br label %prepare_0
            prepare_0:
              call void @__quantum__qis__m__body(ptr inttoptr (i64 0 to ptr), ptr inttoptr (i64 0 to ptr))
              %r_0 = call i1 @__quantum__rt__read_result(ptr inttoptr (i64 0 to ptr))
              br i1 %r_0, label %prepare_0, label %continue_0
            continue_0:
              call void @__quantum__rt__array_record_output(i64 1, ptr null)
              call void @__quantum__rt__result_record_output(ptr inttoptr (i64 0 to ptr), ptr null)
              ret i64 0
            }

            declare void @__quantum__rt__result_record_output(ptr, ptr)
            declare void @__quantum__rt__array_record_output(i64, ptr)
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
fn long_prepare_block() {
    let source = "
PREPARE {
    X 0
    M 0
    H 1
    X 1
    M 1
    M 2
    REQUIRE rec[-1] rec[-2] rec[-3]
}
";
    check(
        source,
        &expect![[r#"
            define i64 @ENTRYPOINT__main() #0 {
              call void @__quantum__rt__initialize(ptr null)
              br label %prepare_0
            prepare_0:
              call void @__quantum__qis__x__body(ptr inttoptr (i64 0 to ptr))
              call void @__quantum__qis__m__body(ptr inttoptr (i64 0 to ptr), ptr inttoptr (i64 0 to ptr))
              call void @__quantum__qis__h__body(ptr inttoptr (i64 1 to ptr))
              call void @__quantum__qis__x__body(ptr inttoptr (i64 1 to ptr))
              call void @__quantum__qis__m__body(ptr inttoptr (i64 1 to ptr), ptr inttoptr (i64 1 to ptr))
              call void @__quantum__qis__m__body(ptr inttoptr (i64 2 to ptr), ptr inttoptr (i64 2 to ptr))
              %r_0 = call i1 @__quantum__rt__read_result(ptr inttoptr (i64 2 to ptr))
              %r_1 = call i1 @__quantum__rt__read_result(ptr inttoptr (i64 1 to ptr))
              %r_2 = call i1 @__quantum__rt__read_result(ptr inttoptr (i64 0 to ptr))
              %x_0 = xor i1 %r_0, %r_1
              %x_1 = xor i1 %x_0, %r_2
              br i1 %x_1, label %prepare_0, label %continue_0
            continue_0:
              call void @__quantum__rt__array_record_output(i64 3, ptr null)
              call void @__quantum__rt__result_record_output(ptr inttoptr (i64 0 to ptr), ptr null)
              call void @__quantum__rt__result_record_output(ptr inttoptr (i64 1 to ptr), ptr null)
              call void @__quantum__rt__result_record_output(ptr inttoptr (i64 2 to ptr), ptr null)
              ret i64 0
            }

            declare void @__quantum__qis__h__body(ptr)
            declare void @__quantum__rt__array_record_output(i64, ptr)
            declare void @__quantum__rt__result_record_output(ptr, ptr)
            declare void @__quantum__qis__x__body(ptr)
            declare i1 @__quantum__rt__read_result(ptr)
            declare void @__quantum__rt__initialize(ptr)
            declare void @__quantum__qis__m__body(ptr, ptr)

            attributes #0 = { "entry_point" "output_labeling_schema" "qir_profiles"="adaptive_profile" "required_num_qubits"="3" "required_num_results"="3" }
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
fn multiple_requires_in_block() {
    let source = "
PREPARE {
    M 0
    REQUIRE rec[-1]
    M 1
    REQUIRE rec[-1] rec[-2]
}
";
    check(
        source,
        &expect![[r#"
            define i64 @ENTRYPOINT__main() #0 {
              call void @__quantum__rt__initialize(ptr null)
              br label %prepare_0
            prepare_0:
              call void @__quantum__qis__m__body(ptr inttoptr (i64 0 to ptr), ptr inttoptr (i64 0 to ptr))
              %r_0 = call i1 @__quantum__rt__read_result(ptr inttoptr (i64 0 to ptr))
              br i1 %r_0, label %prepare_0, label %continue_0
            continue_0:
              call void @__quantum__qis__m__body(ptr inttoptr (i64 1 to ptr), ptr inttoptr (i64 1 to ptr))
              %r_1 = call i1 @__quantum__rt__read_result(ptr inttoptr (i64 1 to ptr))
              %r_2 = call i1 @__quantum__rt__read_result(ptr inttoptr (i64 0 to ptr))
              %x_0 = xor i1 %r_1, %r_2
              br i1 %x_0, label %prepare_0, label %continue_1
            continue_1:
              call void @__quantum__rt__array_record_output(i64 2, ptr null)
              call void @__quantum__rt__result_record_output(ptr inttoptr (i64 0 to ptr), ptr null)
              call void @__quantum__rt__result_record_output(ptr inttoptr (i64 1 to ptr), ptr null)
              ret i64 0
            }

            declare void @__quantum__rt__result_record_output(ptr, ptr)
            declare void @__quantum__rt__array_record_output(i64, ptr)
            declare i1 @__quantum__rt__read_result(ptr)
            declare void @__quantum__rt__initialize(ptr)
            declare void @__quantum__qis__m__body(ptr, ptr)

            attributes #0 = { "entry_point" "output_labeling_schema" "qir_profiles"="adaptive_profile" "required_num_qubits"="2" "required_num_results"="2" }
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
fn prepare_block_no_require() {
    // should compile to a QIR that works as if there was no prepare
    let source = "
PREPARE {
    M 0
    M 1
    M 2
}
";
    check(
        source,
        &expect![[r#"
            define i64 @ENTRYPOINT__main() #0 {
              call void @__quantum__rt__initialize(ptr null)
              br label %prepare_0
            prepare_0:
              call void @__quantum__qis__m__body(ptr inttoptr (i64 0 to ptr), ptr inttoptr (i64 0 to ptr))
              call void @__quantum__qis__m__body(ptr inttoptr (i64 1 to ptr), ptr inttoptr (i64 1 to ptr))
              call void @__quantum__qis__m__body(ptr inttoptr (i64 2 to ptr), ptr inttoptr (i64 2 to ptr))
              call void @__quantum__rt__array_record_output(i64 3, ptr null)
              call void @__quantum__rt__result_record_output(ptr inttoptr (i64 0 to ptr), ptr null)
              call void @__quantum__rt__result_record_output(ptr inttoptr (i64 1 to ptr), ptr null)
              call void @__quantum__rt__result_record_output(ptr inttoptr (i64 2 to ptr), ptr null)
              ret i64 0
            }

            declare void @__quantum__rt__result_record_output(ptr, ptr)
            declare void @__quantum__rt__array_record_output(i64, ptr)
            declare void @__quantum__rt__initialize(ptr)
            declare void @__quantum__qis__m__body(ptr, ptr)

            attributes #0 = { "entry_point" "output_labeling_schema" "qir_profiles"="adaptive_profile" "required_num_qubits"="3" "required_num_results"="3" }
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
fn empty_prepare_block() {
    let source = "
PREPARE {
}
";
    check(
        source,
        &expect![[r#"
            define i64 @ENTRYPOINT__main() #0 {
              call void @__quantum__rt__initialize(ptr null)
              br label %prepare_0
            prepare_0:
              call void @__quantum__rt__array_record_output(i64 0, ptr null)
              ret i64 0
            }

            declare void @__quantum__rt__array_record_output(i64, ptr)
            declare void @__quantum__rt__initialize(ptr)
            declare void @__quantum__rt__result_record_output(ptr, ptr)

            attributes #0 = { "entry_point" "output_labeling_schema" "qir_profiles"="adaptive_profile" "required_num_qubits"="0" "required_num_results"="0" }
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
fn prepare_block_with_args_yields_error() {
    let source = "
PREPARE(0.5) {
    M 0
    REQUIRE rec[-1]
}
";
    check(
        source,
        &expect![[r#"
            Qdk.Stim.Compiler.UnsupportedArgument

              x unsupported argument in instruction: PREPARE
               ,-[2:1]
             1 |
             2 | PREPARE(0.5) {
               : ^^^^^^^^^^^^
             3 |     M 0
               `----
        "#]],
    );
}

#[test]
fn prepare_block_with_targets_yields_error() {
    let source = "
PREPARE 0 1 {
    M 0
    REQUIRE rec[-1]
}
";
    check(
        source,
        &expect![[r#"
            Qdk.Stim.Compiler.UnsupportedTarget

              x unsupported target in instruction: PREPARE
               ,-[2:9]
             1 |
             2 | PREPARE 0 1 {
               :         ^
             3 |     M 0
               `----
        "#]],
    );
}

#[test]
fn prepare_block_with_tag() {
    let source = "
PREPARE[some_tag] {
    M 0
    REQUIRE rec[-1]
}
";
    check(
        source,
        &expect![[r#"
            define i64 @ENTRYPOINT__main() #0 {
              call void @__quantum__rt__initialize(ptr null)
              br label %prepare_0
            prepare_0:
              call void @__quantum__qis__m__body(ptr inttoptr (i64 0 to ptr), ptr inttoptr (i64 0 to ptr))
              %r_0 = call i1 @__quantum__rt__read_result(ptr inttoptr (i64 0 to ptr))
              br i1 %r_0, label %prepare_0, label %continue_0
            continue_0:
              call void @__quantum__rt__array_record_output(i64 1, ptr null)
              call void @__quantum__rt__result_record_output(ptr inttoptr (i64 0 to ptr), ptr null)
              ret i64 0
            }

            declare void @__quantum__rt__result_record_output(ptr, ptr)
            declare void @__quantum__rt__array_record_output(i64, ptr)
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
fn require_with_negated_target() {
    let source = "
PREPARE {
    M 0
    REQUIRE !rec[-1]
}
";
    check(
        source,
        &expect![[r#"
            define i64 @ENTRYPOINT__main() #0 {
              call void @__quantum__rt__initialize(ptr null)
              br label %prepare_0
            prepare_0:
              call void @__quantum__qis__m__body(ptr inttoptr (i64 0 to ptr), ptr inttoptr (i64 0 to ptr))
              %r_0 = call i1 @__quantum__rt__read_result(ptr inttoptr (i64 0 to ptr))
              %r_1 = xor i1 %r_0, true
              br i1 %r_1, label %prepare_0, label %continue_0
            continue_0:
              call void @__quantum__rt__array_record_output(i64 1, ptr null)
              call void @__quantum__rt__result_record_output(ptr inttoptr (i64 0 to ptr), ptr null)
              ret i64 0
            }

            declare void @__quantum__rt__result_record_output(ptr, ptr)
            declare void @__quantum__rt__array_record_output(i64, ptr)
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
fn require_with_integer_target_yields_error() {
    let source = "
PREPARE {
    M 0
    REQUIRE 0
}
";
    check(
        source,
        &expect![[r#"
            Qdk.Stim.Compiler.UnsupportedTarget

              x unsupported target in instruction: REQUIRE
               ,-[4:13]
             3 |     M 0
             4 |     REQUIRE 0
               :             ^
             5 | }
               `----
        "#]],
    );
}

#[test]
fn require_with_pauli_target_yields_error() {
    let source = "
PREPARE {
    M 0
    REQUIRE X0
}
";
    check(
        source,
        &expect![[r#"
            Qdk.Stim.Compiler.UnsupportedTarget

              x unsupported target in instruction: REQUIRE
               ,-[4:13]
             3 |     M 0
             4 |     REQUIRE X0
               :             ^^
             5 | }
               `----
        "#]],
    );
}

#[test]
fn require_with_no_targets_yields_error() {
    let source = "
PREPARE {
    M 0
    REQUIRE
}
";
    check(
        source,
        &expect![[r#"
            Qdk.Stim.Compiler.UnsupportedTarget

              x unsupported target in instruction: REQUIRE
               ,-[4:5]
             3 |     M 0
             4 |     REQUIRE
               :     ^^^^^^^
             5 | }
               `----
        "#]],
    );
}

#[test]
fn require_no_prepare_block_yields_error() {
    let source = "
REQUIRE rec[-1]
";
    check(
        source,
        &expect![[r#"
            Qdk.Stim.Compiler.RequireOutsidePrepareBlock

              x require must appear inside a PREPARE block
               ,-[2:1]
             1 |
             2 | REQUIRE rec[-1]
               : ^^^^^^^^^^^^^^^
               `----
        "#]],
    );
}

#[test]
fn require_with_no_measurements_yields_error() {
    let source = "
PREPARE {
    REQUIRE rec[-1]
}
";
    check(
        source,
        &expect![[r#"
            Qdk.Stim.Compiler.MeasurementRecordOutOfBounds

              x measurement record is out of bounds
               ,-[3:13]
             2 | PREPARE {
             3 |     REQUIRE rec[-1]
               :             ^^^^^^^
             4 | }
               `----
        "#]],
    );
}

#[test]
fn require_before_measurement_yields_error() {
    let source = "
PREPARE {
    REQUIRE rec[-1]
    M 0
}
";
    check(
        source,
        &expect![[r#"
            Qdk.Stim.Compiler.MeasurementRecordOutOfBounds

              x measurement record is out of bounds
               ,-[3:13]
             2 | PREPARE {
             3 |     REQUIRE rec[-1]
               :             ^^^^^^^
             4 |     M 0
               `----
        "#]],
    );
}

#[test]
fn rec_index_out_of_bounds() {
    let source = "
PREPARE {
    M 0
    REQUIRE rec[-2]
}
";
    check(
        source,
        &expect![[r#"
            Qdk.Stim.Compiler.MeasurementRecordOutOfBounds

              x measurement record is out of bounds
               ,-[4:13]
             3 |     M 0
             4 |     REQUIRE rec[-2]
               :             ^^^^^^^
             5 | }
               `----
        "#]],
    );
}

#[test]
fn rec_index_out_of_scope() {
    let source = "
M 0
PREPARE {
    M 1
    REQUIRE rec[-2]
}
";
    check(
        source,
        &expect![[r#"
            Qdk.Stim.Compiler.MeasurementRecordOutOfScope

              x measurement record refers to a measurement outside the enclosing PREPARE
              | block
               ,-[5:13]
             4 |     M 1
             5 |     REQUIRE rec[-2]
               :             ^^^^^^^
             6 | }
               `----
        "#]],
    );
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
    check(
        source,
        &expect![[r#"
            Qdk.Stim.Compiler.MeasurementRecordOutOfBounds

              x measurement record is out of bounds
               ,-[4:13]
             3 |     R 0
             4 |     REQUIRE rec[-1]
               :             ^^^^^^^
             5 | }
               `----
        "#]],
    );
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
    check(
        source,
        &expect![[r#"
            define i64 @ENTRYPOINT__main() #0 {
              call void @__quantum__rt__initialize(ptr null)
              br label %prepare_0
            prepare_0:
              call void @__quantum__qis__mresetz__body(ptr inttoptr (i64 0 to ptr), ptr inttoptr (i64 0 to ptr))
              %r_0 = call i1 @__quantum__rt__read_result(ptr inttoptr (i64 0 to ptr))
              br i1 %r_0, label %prepare_0, label %continue_0
            continue_0:
              call void @__quantum__rt__array_record_output(i64 1, ptr null)
              call void @__quantum__rt__result_record_output(ptr inttoptr (i64 0 to ptr), ptr null)
              ret i64 0
            }

            declare void @__quantum__rt__result_record_output(ptr, ptr)
            declare void @__quantum__rt__array_record_output(i64, ptr)
            declare i1 @__quantum__rt__read_result(ptr)
            declare void @__quantum__rt__initialize(ptr)
            declare void @__quantum__qis__mresetz__body(ptr, ptr)

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
fn pair_measurement_record_in_prepare() {
    let source = "
PREPARE {
    MZZ 0 1
    REQUIRE rec[-1]
}
";
    check(
        source,
        &expect![[r#"
            define i64 @ENTRYPOINT__main() #0 {
              call void @__quantum__rt__initialize(ptr null)
              br label %prepare_0
            prepare_0:
              call void @__quantum__qis__cx__body(ptr inttoptr (i64 0 to ptr), ptr inttoptr (i64 1 to ptr))
              call void @__quantum__qis__m__body(ptr inttoptr (i64 1 to ptr), ptr inttoptr (i64 0 to ptr))
              call void @__quantum__qis__cx__body(ptr inttoptr (i64 0 to ptr), ptr inttoptr (i64 1 to ptr))
              %r_0 = call i1 @__quantum__rt__read_result(ptr inttoptr (i64 0 to ptr))
              br i1 %r_0, label %prepare_0, label %continue_0
            continue_0:
              call void @__quantum__rt__array_record_output(i64 1, ptr null)
              call void @__quantum__rt__result_record_output(ptr inttoptr (i64 0 to ptr), ptr null)
              ret i64 0
            }

            declare void @__quantum__qis__cx__body(ptr, ptr)
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
fn pair_measurement_record_in_prepare_out_of_bounds() {
    // A two-qubit measurement produces a single measurement record.
    // So this should not be valid
    let source = "
PREPARE {
    MZZ 0 1
    REQUIRE rec[-1] rec[-2]
}
";
    check(
        source,
        &expect![[r#"
            Qdk.Stim.Compiler.MeasurementRecordOutOfBounds

              x measurement record is out of bounds
               ,-[4:21]
             3 |     MZZ 0 1
             4 |     REQUIRE rec[-1] rec[-2]
               :                     ^^^^^^^
             5 | }
               `----
        "#]],
    );
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
    check(
        source,
        &expect![[r#"
            define i64 @ENTRYPOINT__main() #0 {
              call void @__quantum__rt__initialize(ptr null)
              br label %prepare_0
            prepare_0:
              br label %prepare_1
            prepare_1:
              call void @__quantum__qis__m__body(ptr inttoptr (i64 0 to ptr), ptr inttoptr (i64 0 to ptr))
              %r_0 = call i1 @__quantum__rt__read_result(ptr inttoptr (i64 0 to ptr))
              br i1 %r_0, label %prepare_1, label %continue_0
            continue_0:
              call void @__quantum__qis__m__body(ptr inttoptr (i64 1 to ptr), ptr inttoptr (i64 1 to ptr))
              %r_1 = call i1 @__quantum__rt__read_result(ptr inttoptr (i64 1 to ptr))
              br i1 %r_1, label %prepare_0, label %continue_1
            continue_1:
              call void @__quantum__rt__array_record_output(i64 2, ptr null)
              call void @__quantum__rt__result_record_output(ptr inttoptr (i64 0 to ptr), ptr null)
              call void @__quantum__rt__result_record_output(ptr inttoptr (i64 1 to ptr), ptr null)
              ret i64 0
            }

            declare void @__quantum__rt__result_record_output(ptr, ptr)
            declare void @__quantum__rt__array_record_output(i64, ptr)
            declare i1 @__quantum__rt__read_result(ptr)
            declare void @__quantum__rt__initialize(ptr)
            declare void @__quantum__qis__m__body(ptr, ptr)

            attributes #0 = { "entry_point" "output_labeling_schema" "qir_profiles"="adaptive_profile" "required_num_qubits"="2" "required_num_results"="2" }
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
    check(
        source,
        &expect![[r#"
            define i64 @ENTRYPOINT__main() #0 {
              call void @__quantum__rt__initialize(ptr null)
              br label %prepare_0
            prepare_0:
              br label %prepare_1
            prepare_1:
              br label %prepare_2
            prepare_2:
              call void @__quantum__qis__m__body(ptr inttoptr (i64 0 to ptr), ptr inttoptr (i64 0 to ptr))
              %r_0 = call i1 @__quantum__rt__read_result(ptr inttoptr (i64 0 to ptr))
              br i1 %r_0, label %prepare_2, label %continue_0
            continue_0:
              call void @__quantum__qis__m__body(ptr inttoptr (i64 1 to ptr), ptr inttoptr (i64 1 to ptr))
              %r_1 = call i1 @__quantum__rt__read_result(ptr inttoptr (i64 1 to ptr))
              br i1 %r_1, label %prepare_1, label %continue_1
            continue_1:
              call void @__quantum__qis__m__body(ptr inttoptr (i64 2 to ptr), ptr inttoptr (i64 2 to ptr))
              %r_2 = call i1 @__quantum__rt__read_result(ptr inttoptr (i64 2 to ptr))
              br i1 %r_2, label %prepare_0, label %continue_2
            continue_2:
              call void @__quantum__rt__array_record_output(i64 3, ptr null)
              call void @__quantum__rt__result_record_output(ptr inttoptr (i64 0 to ptr), ptr null)
              call void @__quantum__rt__result_record_output(ptr inttoptr (i64 1 to ptr), ptr null)
              call void @__quantum__rt__result_record_output(ptr inttoptr (i64 2 to ptr), ptr null)
              ret i64 0
            }

            declare void @__quantum__rt__result_record_output(ptr, ptr)
            declare void @__quantum__rt__array_record_output(i64, ptr)
            declare i1 @__quantum__rt__read_result(ptr)
            declare void @__quantum__rt__initialize(ptr)
            declare void @__quantum__qis__m__body(ptr, ptr)

            attributes #0 = { "entry_point" "output_labeling_schema" "qir_profiles"="adaptive_profile" "required_num_qubits"="3" "required_num_results"="3" }
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
fn outer_prepare_reaches_into_inner_prepare_yields_error() {
    let source = "
PREPARE {
    PREPARE {
        M 0
    }
    REQUIRE rec[-1]
}
";
    check(
        source,
        &expect![[r#"
            Qdk.Stim.Compiler.MeasurementRecordOutOfScope

              x measurement record refers to a measurement outside the enclosing PREPARE
              | block
               ,-[6:13]
             5 |     }
             6 |     REQUIRE rec[-1]
               :             ^^^^^^^
             7 | }
               `----
        "#]],
    );
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
    check(
        source,
        &expect![[r#"
            Qdk.Stim.Compiler.MeasurementRecordOutOfScope

              x measurement record refers to a measurement outside the enclosing PREPARE
              | block
               ,-[5:17]
             4 |     PREPARE {
             5 |         REQUIRE rec[-1]
               :                 ^^^^^^^
             6 |     }
               `----
        "#]],
    );
}

#[test]
fn sibling_prepare_blocks() {
    let source = "
PREPARE {
    M 0
    REQUIRE rec[-1]
}
PREPARE {
    M 1
    REQUIRE rec[-1]
}
";
    check(
        source,
        &expect![[r#"
        define i64 @ENTRYPOINT__main() #0 {
          call void @__quantum__rt__initialize(ptr null)
          br label %prepare_0
        prepare_0:
          call void @__quantum__qis__m__body(ptr inttoptr (i64 0 to ptr), ptr inttoptr (i64 0 to ptr))
          %r_0 = call i1 @__quantum__rt__read_result(ptr inttoptr (i64 0 to ptr))
          br i1 %r_0, label %prepare_0, label %continue_0
        continue_0:
          br label %prepare_1
        prepare_1:
          call void @__quantum__qis__m__body(ptr inttoptr (i64 1 to ptr), ptr inttoptr (i64 1 to ptr))
          %r_1 = call i1 @__quantum__rt__read_result(ptr inttoptr (i64 1 to ptr))
          br i1 %r_1, label %prepare_1, label %continue_1
        continue_1:
          call void @__quantum__rt__array_record_output(i64 2, ptr null)
          call void @__quantum__rt__result_record_output(ptr inttoptr (i64 0 to ptr), ptr null)
          call void @__quantum__rt__result_record_output(ptr inttoptr (i64 1 to ptr), ptr null)
          ret i64 0
        }

        declare void @__quantum__rt__result_record_output(ptr, ptr)
        declare void @__quantum__rt__array_record_output(i64, ptr)
        declare i1 @__quantum__rt__read_result(ptr)
        declare void @__quantum__rt__initialize(ptr)
        declare void @__quantum__qis__m__body(ptr, ptr)

        attributes #0 = { "entry_point" "output_labeling_schema" "qir_profiles"="adaptive_profile" "required_num_qubits"="2" "required_num_results"="2" }
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
fn sibling_prepare_block_cannot_require_previous_block_measurement_yields_error() {
    let source = "
PREPARE {
    M 0
}
PREPARE {
    M 1
    REQUIRE rec[-2]
}
";
    check(
        source,
        &expect![[r#"
            Qdk.Stim.Compiler.MeasurementRecordOutOfScope

              x measurement record refers to a measurement outside the enclosing PREPARE
              | block
               ,-[7:13]
             6 |     M 1
             7 |     REQUIRE rec[-2]
               :             ^^^^^^^
             8 | }
               `----
        "#]],
    );
}

#[test]
fn blockless_prepare_yields_error() {
    let source = "
X 0
PREPARE
M 0
";
    check(
        source,
        &expect![[r#"
            Qdk.Stim.Compiler.PrepareWithoutBlock

              x prepare instruction must start a block
               ,-[3:1]
             2 | X 0
             3 | PREPARE
               : ^^^^^^^
             4 | M 0
               `----
        "#]],
    );
}
