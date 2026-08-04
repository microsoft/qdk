// Copyright (c) Microsoft Corporation.
// Licensed under the MIT License.

use crate::qir::tests::check;
use expect_test::expect;
use indoc::indoc;

#[test]
fn peek_loss_single_qubit() {
    check(
        "PEEK_LOSS 0",
        &expect![[r#"
        define i64 @ENTRYPOINT__main() #0 {
          call void @__quantum__rt__initialize(ptr null)
          call void @__quantum__qis__peek_loss__body(ptr inttoptr (i64 0 to ptr), ptr inttoptr (i64 0 to ptr))
          call void @__quantum__rt__array_record_output(i64 1, ptr null)
          call void @__quantum__rt__result_record_output(ptr inttoptr (i64 0 to ptr), ptr null)
          ret i64 0
        }

        declare void @__quantum__rt__result_record_output(ptr, ptr)
        declare void @__quantum__rt__array_record_output(i64, ptr)
        declare void @__quantum__rt__initialize(ptr)
        declare void @__quantum__qis__peek_loss__body(ptr, ptr)

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
fn peek_loss_broadcasts_over_multiple_qubits() {
    check(
        "PEEK_LOSS 0 1 2",
        &expect![[r#"
        define i64 @ENTRYPOINT__main() #0 {
          call void @__quantum__rt__initialize(ptr null)
          call void @__quantum__qis__peek_loss__body(ptr inttoptr (i64 0 to ptr), ptr inttoptr (i64 0 to ptr))
          call void @__quantum__qis__peek_loss__body(ptr inttoptr (i64 1 to ptr), ptr inttoptr (i64 1 to ptr))
          call void @__quantum__qis__peek_loss__body(ptr inttoptr (i64 2 to ptr), ptr inttoptr (i64 2 to ptr))
          call void @__quantum__rt__array_record_output(i64 3, ptr null)
          call void @__quantum__rt__result_record_output(ptr inttoptr (i64 0 to ptr), ptr null)
          call void @__quantum__rt__result_record_output(ptr inttoptr (i64 1 to ptr), ptr null)
          call void @__quantum__rt__result_record_output(ptr inttoptr (i64 2 to ptr), ptr null)
          ret i64 0
        }

        declare void @__quantum__rt__result_record_output(ptr, ptr)
        declare void @__quantum__rt__array_record_output(i64, ptr)
        declare void @__quantum__rt__initialize(ptr)
        declare void @__quantum__qis__peek_loss__body(ptr, ptr)

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

// temporary, until we start accepting arguments to measurements
#[test]
fn peek_loss_with_args_yields_error() {
    check(
        "PEEK_LOSS(0.5) 0",
        &expect![[r#"
        Qdk.Stim.Compiler.UnsupportedArgument

          x unsupported argument in instruction: PEEK_LOSS
           ,----
         1 | PEEK_LOSS(0.5) 0
           : ^^^^^^^^^^^^^^^^
           `----
    "#]],
    );
}

#[test]
fn peek_loss_with_negated_target_yields_error() {
    check(
        "PEEK_LOSS !0",
        &expect![[r#"
            Qdk.Stim.Compiler.NegatedTarget

              x target cannot be negated in instruction: PEEK_LOSS
               ,----
             1 | PEEK_LOSS !0
               :           ^^
               `----
        "#]],
    );
}

#[test]
fn peek_loss_referenced_by_classical_control() {
    let source = indoc! {"
        PEEK_LOSS 0
        CX rec[-1] 1
    "};
    check(
        source,
        &expect![[r#"
        define i64 @ENTRYPOINT__main() #0 {
          call void @__quantum__rt__initialize(ptr null)
          call void @__quantum__qis__peek_loss__body(ptr inttoptr (i64 0 to ptr), ptr inttoptr (i64 0 to ptr))
          call void @classical_control_cx(ptr inttoptr (i64 0 to ptr), ptr inttoptr (i64 1 to ptr))
          call void @__quantum__rt__array_record_output(i64 1, ptr null)
          call void @__quantum__rt__result_record_output(ptr inttoptr (i64 0 to ptr), ptr null)
          ret i64 0
        }

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

        declare void @__quantum__rt__array_record_output(i64, ptr)
        declare void @__quantum__rt__result_record_output(ptr, ptr)
        declare void @__quantum__qis__x__body(ptr)
        declare i1 @__quantum__rt__read_result(ptr)
        declare void @__quantum__rt__initialize(ptr)
        declare void @__quantum__qis__peek_loss__body(ptr, ptr)

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
fn peek_loss_referenced_by_notleaked_in_select_block_yields_error() {
    let source = indoc! {"
        SELECT {
          PEEK_LOSS 0
          NOTLEAKED rec[-1]
        }
    "};
    check(
        source,
        &expect![[r#"
        Qdk.Stim.Compiler.NotLeakedOnPeekLoss

          x NOTLEAKED cannot reference a record produced by PEEK_LOSS
           ,-[3:13]
         2 |   PEEK_LOSS 0
         3 |   NOTLEAKED rec[-1]
           :             ^^^^^^^
         4 | }
           `----
    "#]],
    );
}

#[test]
fn peek_loss_referenced_by_require_in_select_block() {
    let source = indoc! {"
        SELECT {
          PEEK_LOSS 0
          REQUIRE rec[-1]
        }
    "};
    check(
        source,
        &expect![[r#"
        define i64 @ENTRYPOINT__main() #0 {
          call void @__quantum__rt__initialize(ptr null)
          br label %select_0
        select_0:
          call void @__quantum__qis__peek_loss__body(ptr inttoptr (i64 0 to ptr), ptr inttoptr (i64 0 to ptr))
          %l_0 = call i1 @__quantum__rt__read_loss(ptr inttoptr (i64 0 to ptr))
          %r_0 = call i1 @__quantum__rt__read_result(ptr inttoptr (i64 0 to ptr))
          %restart_0 = or i1 %l_0, %r_0
          br i1 %restart_0, label %select_0, label %continue_0
        continue_0:
          call void @__quantum__rt__array_record_output(i64 1, ptr null)
          call void @__quantum__rt__result_record_output(ptr inttoptr (i64 0 to ptr), ptr null)
          ret i64 0
        }

        declare void @__quantum__rt__array_record_output(i64, ptr)
        declare void @__quantum__rt__result_record_output(ptr, ptr)
        declare i1 @__quantum__rt__read_loss(ptr)
        declare i1 @__quantum__rt__read_result(ptr)
        declare void @__quantum__rt__initialize(ptr)
        declare void @__quantum__qis__peek_loss__body(ptr, ptr)

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
fn peek_loss_interleaved_with_measurements() {
    let source = indoc! {"
        M 0
        PEEK_LOSS 1
        M 2
    "};
    check(
        source,
        &expect![[r#"
        define i64 @ENTRYPOINT__main() #0 {
          call void @__quantum__rt__initialize(ptr null)
          call void @__quantum__qis__m__body(ptr inttoptr (i64 0 to ptr), ptr inttoptr (i64 0 to ptr))
          call void @__quantum__qis__peek_loss__body(ptr inttoptr (i64 1 to ptr), ptr inttoptr (i64 1 to ptr))
          call void @__quantum__qis__m__body(ptr inttoptr (i64 2 to ptr), ptr inttoptr (i64 2 to ptr))
          call void @__quantum__rt__array_record_output(i64 3, ptr null)
          call void @__quantum__rt__result_record_output(ptr inttoptr (i64 0 to ptr), ptr null)
          call void @__quantum__rt__result_record_output(ptr inttoptr (i64 1 to ptr), ptr null)
          call void @__quantum__rt__result_record_output(ptr inttoptr (i64 2 to ptr), ptr null)
          ret i64 0
        }

        declare void @__quantum__qis__m__body(ptr, ptr)
        declare void @__quantum__rt__result_record_output(ptr, ptr)
        declare void @__quantum__rt__array_record_output(i64, ptr)
        declare void @__quantum__qis__peek_loss__body(ptr, ptr)
        declare void @__quantum__rt__initialize(ptr)

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
fn notleaked_errors_on_peek_mixed_with_measurement() {
    let source = indoc! {"
        SELECT {
          PEEK_LOSS 0
          M 1
          NOTLEAKED rec[-1] rec[-2]
        }
    "};
    check(
        source,
        &expect![[r#"
        Qdk.Stim.Compiler.NotLeakedOnPeekLoss

          x NOTLEAKED cannot reference a record produced by PEEK_LOSS
           ,-[4:21]
         3 |   M 1
         4 |   NOTLEAKED rec[-1] rec[-2]
           :                     ^^^^^^^
         5 | }
           `----
    "#]],
    );
}

#[test]
fn require_allows_peek_record_mixed_with_measurement() {
    let source = indoc! {"
        SELECT {
          PEEK_LOSS 0
          M 1
          REQUIRE rec[-1] rec[-2]
        }
    "};
    check(
        source,
        &expect![[r#"
        define i64 @ENTRYPOINT__main() #0 {
          call void @__quantum__rt__initialize(ptr null)
          br label %select_0
        select_0:
          call void @__quantum__qis__peek_loss__body(ptr inttoptr (i64 0 to ptr), ptr inttoptr (i64 0 to ptr))
          call void @__quantum__qis__m__body(ptr inttoptr (i64 1 to ptr), ptr inttoptr (i64 1 to ptr))
          %l_0 = call i1 @__quantum__rt__read_loss(ptr inttoptr (i64 1 to ptr))
          %r_0 = call i1 @__quantum__rt__read_result(ptr inttoptr (i64 1 to ptr))
          %l_1 = call i1 @__quantum__rt__read_loss(ptr inttoptr (i64 0 to ptr))
          %r_1 = call i1 @__quantum__rt__read_result(ptr inttoptr (i64 0 to ptr))
          %loss_0 = or i1 %l_0, %l_1
          %parity_0 = xor i1 %r_0, %r_1
          %restart_0 = or i1 %loss_0, %parity_0
          br i1 %restart_0, label %select_0, label %continue_0
        continue_0:
          call void @__quantum__rt__array_record_output(i64 2, ptr null)
          call void @__quantum__rt__result_record_output(ptr inttoptr (i64 0 to ptr), ptr null)
          call void @__quantum__rt__result_record_output(ptr inttoptr (i64 1 to ptr), ptr null)
          ret i64 0
        }

        declare void @__quantum__qis__peek_loss__body(ptr, ptr)
        declare void @__quantum__rt__array_record_output(i64, ptr)
        declare void @__quantum__rt__result_record_output(ptr, ptr)
        declare i1 @__quantum__rt__read_loss(ptr)
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
