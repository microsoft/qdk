// Copyright (c) Microsoft Corporation.
// Licensed under the MIT License.

use super::check;
use expect_test::expect;

#[test]
#[ignore = "this will be deprecated soon"]
fn preselect_begin_yields_expected_qir() {
    let source = "#!preselect_begin";

    check(
        source,
        &expect![[r#"
        define i64 @ENTRYPOINT__main() #0 {
          call void @__quantum__rt__initialize(ptr null)
          br label %preselect_begin_0
        preselect_begin_0:
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
#[ignore = "this will be deprecated soon"]
fn preselect_begin_followed_by_preselect_expect_yields_expected_qir() {
    let source = "
#!preselect_begin
H 0
M 0
R 0
#!preselect_expect rec[-1] 1
";

    check(
        source,
        &expect![[r#"
            define i64 @ENTRYPOINT__main() #0 {
              call void @__quantum__rt__initialize(ptr null)
              br label %preselect_begin_0
            preselect_begin_0:
              call void @__quantum__qis__h__body(ptr inttoptr (i64 0 to ptr))
              call void @__quantum__qis__m__body(ptr inttoptr (i64 0 to ptr), ptr inttoptr (i64 0 to ptr))
              call void @__quantum__qis__reset__body(ptr inttoptr (i64 0 to ptr))
              %preselect_r0 = call i1 @__quantum__rt__read_result(ptr inttoptr (i64 0 to ptr))
              br i1 %preselect_r0, label %preselect_continue_0, label %preselect_begin_0
            preselect_continue_0:
              call void @__quantum__rt__array_record_output(i64 1, ptr null)
              call void @__quantum__rt__result_record_output(ptr inttoptr (i64 0 to ptr), ptr null)
              ret i64 0
            }

            declare void @__quantum__qis__h__body(ptr)
            declare void @__quantum__qis__reset__body(ptr)
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
#[ignore = "this will be deprecated soon"]
fn preselect_begin_followed_by_two_preselect_expect_yields_expected_qir() {
    let source = "
#!preselect_begin
H 0 1
M 0 1
R 0 1
#!preselect_expect rec[-2] 1
#!preselect_expect rec[-1] 1
";

    check(
        source,
        &expect![[r#"
            define i64 @ENTRYPOINT__main() #0 {
              call void @__quantum__rt__initialize(ptr null)
              br label %preselect_begin_0
            preselect_begin_0:
              call void @__quantum__qis__h__body(ptr inttoptr (i64 0 to ptr))
              call void @__quantum__qis__h__body(ptr inttoptr (i64 1 to ptr))
              call void @__quantum__qis__m__body(ptr inttoptr (i64 0 to ptr), ptr inttoptr (i64 0 to ptr))
              call void @__quantum__qis__m__body(ptr inttoptr (i64 1 to ptr), ptr inttoptr (i64 1 to ptr))
              call void @__quantum__qis__reset__body(ptr inttoptr (i64 0 to ptr))
              call void @__quantum__qis__reset__body(ptr inttoptr (i64 1 to ptr))
              %preselect_r0 = call i1 @__quantum__rt__read_result(ptr inttoptr (i64 0 to ptr))
              br i1 %preselect_r0, label %preselect_continue_0, label %preselect_begin_0
            preselect_continue_0:
              %preselect_r1 = call i1 @__quantum__rt__read_result(ptr inttoptr (i64 1 to ptr))
              br i1 %preselect_r1, label %preselect_continue_0, label %preselect_begin_0
            preselect_continue_0:
              call void @__quantum__rt__array_record_output(i64 2, ptr null)
              call void @__quantum__rt__result_record_output(ptr inttoptr (i64 0 to ptr), ptr null)
              call void @__quantum__rt__result_record_output(ptr inttoptr (i64 1 to ptr), ptr null)
              ret i64 0
            }

            declare void @__quantum__qis__h__body(ptr)
            declare void @__quantum__qis__reset__body(ptr)
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
